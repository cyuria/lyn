use smithay::{
    desktop::{
        Space,
        Window,
    },
    input::pointer::{
        AxisFrame,
        ButtonEvent,
        GestureHoldBeginEvent,
        GestureHoldEndEvent,
        GesturePinchBeginEvent,
        GesturePinchEndEvent,
        GesturePinchUpdateEvent,
        GestureSwipeBeginEvent,
        GestureSwipeEndEvent,
        GestureSwipeUpdateEvent,
        GrabStartData as PointerGrabStartData,
        MotionEvent,
        PointerGrab,
        PointerInnerHandle,
        RelativeMotionEvent,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{
        Logical,
        Point,
        Rectangle,
        Size,
    },
    wayland::{
        compositor,
        shell::xdg::SurfaceCachedState,
    },
};
use crate::LynWM;
use std::cell::RefCell;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ResizeEdge: u32 {
        const TOP    = 0b0001;
        const BOTTOM = 0b0010;
        const LEFT   = 0b0100;
        const RIGHT  = 0b1000;
        
        const TOP_LEFT     = Self::TOP.bits()    | Self::LEFT.bits();
        const BOTTOM_LEFT  = Self::BOTTOM.bits() | Self::LEFT.bits();
        const TOP_RIGHT    = Self::TOP.bits()    | Self::RIGHT.bits();
        const BOTTOM_RIGHT = Self::BOTTOM.bits() | Self::RIGHT.bits();
    }
}

impl From<xdg_toplevel::ResizeEdge> for ResizeEdge {
    #[inline]
    fn from(value: xdg_toplevel::ResizeEdge) -> Self {
        Self::from_bits(value as u32).unwrap()
    }
}

pub struct ResizeSurfaceGrab {
    start_data: PointerGrabStartData<LynWM>,
    window: Window,
    edges: ResizeEdge,
    initial_rect: Rectangle<i32, Logical>,
    last_window_size: Size<i32, Logical>,
}

impl ResizeSurfaceGrab {
    pub fn start(
        start_data: PointerGrabStartData<LynWM>,
        window: Window,
        edges: ResizeEdge,
        initial_window_rect: Rectangle<i32, Logical>,
    ) -> Self {
        let initial_rect = initial_window_rect;

        ResizeSurfaceState::with(window.toplevel().unwrap().wl_surface(), |state| {
            *state = ResizeSurfaceState::Resizing { edges, initial_rect };
        });

        Self {
            start_data,
            window,
            edges,
            initial_rect,
            last_window_size: initial_rect.size,
        }
    }
}

impl PointerGrab<LynWM> for ResizeSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        _focus: Option<(<LynWM as smithay::input::SeatHandler>::PointerFocus, Point<i32, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);

        let delta = event.location - self.start_data.location;

        let mut width = self.initial_rect.size.w;
        let mut height = self.initial_rect.size.h;

        if self.edges.intersects(ResizeEdge::LEFT) {
            width = (width as f64 - delta.x) as i32;
        } else if self.edges.intersects(ResizeEdge::RIGHT) {
            width = (width as f64 + delta.x) as i32;
        }

        if self.edges.intersects(ResizeEdge::TOP) {
            height = (height as f64 - delta.y) as i32;
        } else if self.edges.intersects(ResizeEdge::BOTTOM) {
            height = (height as f64 + delta.y) as i32;
        }

        let (min_size, max_size) =
            compositor::with_states(self.window.toplevel().unwrap().wl_surface(), |states| {
                let data = states.cached_state.current::<SurfaceCachedState>();
                (data.min_size, data.max_size)
            });

        let min_width = min_size.w.max(1);
        let min_height = min_size.h.max(1);

        let max_width = if max_size.w != 0 { max_size.w } else { i32::MAX };
        let max_height = if max_size.h != 0 { max_size.h } else { i32::MAX };

        self.last_window_size = Size::from((
            width.max(min_width).min(max_width),
            height.max(min_height).min(max_height),
        ));

        let xdg = self.window.toplevel().unwrap();
        xdg.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
            state.size = Some(self.last_window_size);
        });
        xdg.send_pending_configure();
    }

    fn relative_motion(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        focus: Option<(<LynWM as smithay::input::SeatHandler>::PointerFocus, Point<i32, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(&mut self, data: &mut LynWM, handle: &mut PointerInnerHandle<'_, LynWM>, event: &ButtonEvent) {
        handle.button(data, event);

        // from <linux/input-event-codes.h>
        const BTN_LEFT: u32 = 0x110;

        if handle.current_pressed().contains(&BTN_LEFT) {
            return;
        }

        handle.unset_grab(self, data, event.serial, event.time, true);

        let xdg = self.window.toplevel().unwrap();
        xdg.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Resizing);
            state.size = Some(self.last_window_size);
        });

        xdg.send_pending_configure();

        ResizeSurfaceState::with(xdg.wl_surface(), |state| {
            *state = ResizeSurfaceState::WaitingForLastCommit {
                edges: self.edges,
                initial_rect: self.initial_rect,
            };
        });
    }

    fn axis(&mut self, data: &mut LynWM, handle: &mut PointerInnerHandle<'_, LynWM>, details: AxisFrame) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut LynWM, handle: &mut PointerInnerHandle<'_, LynWM>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut LynWM,
        handle: &mut PointerInnerHandle<'_, LynWM>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &PointerGrabStartData<LynWM> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut LynWM) {}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
enum ResizeSurfaceState {
    #[default]
    Idle,
    Resizing {
        edges: ResizeEdge,
        initial_rect: Rectangle<i32, Logical>,
    },
    WaitingForLastCommit {
        edges: ResizeEdge,
        initial_rect: Rectangle<i32, Logical>,
    },
}

impl ResizeSurfaceState {
    fn with<F, T>(surface: &WlSurface, cb: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
    {
        compositor::with_states(surface, |states| {
            states.data_map.insert_if_missing(RefCell::<Self>::default);
            let state = states.data_map.get::<RefCell<Self>>().unwrap();

            cb(&mut state.borrow_mut())
        })
    }

    fn commit(&mut self) -> Option<(ResizeEdge, Rectangle<i32, Logical>)> {
        match *self {
            Self::Resizing { edges, initial_rect } => Some((edges, initial_rect)),
            Self::WaitingForLastCommit { edges, initial_rect } => {
                *self = Self::Idle;
                Some((edges, initial_rect))
            }
            Self::Idle => None,
        }
    }
}

pub fn handle_commit(space: &mut Space<Window>, surface: &WlSurface) -> Option<()> {
    let window = space
        .elements()
        .find(|w| w.toplevel().unwrap().wl_surface() == surface)
        .cloned()?;

    let mut window_location = space.element_location(&window)?;
    let geometry = window.geometry();

    let location: Point<Option<i32>, Logical> = ResizeSurfaceState::with(surface, |state| {
        let Some((edges, initial_rect)) = state.commit() else {
            return Default::default();
        };

        if !edges.intersects(ResizeEdge::TOP_LEFT) {
            return (None, None).into();
        }

        let x = edges
            .intersects(ResizeEdge::LEFT)
            .then_some(initial_rect.loc.x + initial_rect.size.w - geometry.size.w);

        let y = edges
            .intersects(ResizeEdge::TOP)
            .then_some(initial_rect.loc.y + initial_rect.size.h - geometry.size.h);

        (x, y).into()
    });

    if let Some(x) = location.x {
        window_location.x = x;
    }
    if let Some(y) = location.y {
        window_location.y = y;
    }

    if location.x.is_some() || location.y.is_some() {
        space.map_element(window, window_location, false);
    }

    Some(())
}
