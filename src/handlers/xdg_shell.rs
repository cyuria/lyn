
use smithay::{
    delegate_xdg_shell,
    desktop::{
        find_popup_root_surface,
        get_popup_toplevel_coords,
        PopupKind,
        PopupManager,
        Space,
        Window,
    }, input::{
        pointer::{
            Focus,
            GrabStartData as PointerGrabStartData
        },
        Seat,
    }, reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            protocol::{
                wl_seat,
                wl_surface::WlSurface,
            },
            Resource,
        }
    }, utils::{
        Rectangle,
        Serial,
    }, wayland::{
        compositor::with_states,
        shell::xdg::{
            PopupSurface,
            PositionerState,
            ToplevelSurface,
            XdgPopupSurfaceData,
            XdgShellHandler,
            XdgShellState,
            XdgToplevelSurfaceData,
        },
    }
};

use crate::{
    grabs::{
        MoveSurfaceGrab,
        ResizeSurfaceGrab,
    },
    LynWM,
};

impl XdgShellHandler for LynWM {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window, (0, 0), false);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let seat = Seat::from_resource(&seat).unwrap();
        let wl_surface = surface.wl_surface();

        let Some(start_data) = check_grab(&seat, wl_surface, serial) else {
            return;
        };

        let pointer = seat.get_pointer().unwrap();
        let window = self.space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == wl_surface)
            .unwrap()
            .clone();
        let initial_window_location = self.space.element_location(&window).unwrap();

        let grab = MoveSurfaceGrab {
            start_data,
            window,
            initial_window_location,
        };

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let seat = Seat::from_resource(&seat).unwrap();
        let wl_surface = surface.wl_surface();
        
        let Some(start_data) = check_grab(&seat, wl_surface, serial) else {
            return;
        };

        let pointer = seat.get_pointer().unwrap();

        let window = self.space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == wl_surface)
            .unwrap()
            .clone();
        let location = self.space
            .element_location(&window)
            .unwrap();
        let size = window.geometry().size;

        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
        });

        surface.send_pending_configure();

        let grab = ResizeSurfaceGrab::start(
            start_data,
            window,
            edges.into(),
            Rectangle::from_loc_and_size(location, size)
        );

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // The Smallvil source has TODO popup grabs
    }
}

delegate_xdg_shell!(LynWM);

fn check_grab(
    seat: &Seat<LynWM>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<PointerGrabStartData<LynWM>> {
    let pointer = seat.get_pointer()?;

    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;

    if !focus.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}

pub fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
    if let Some(window) = space
        .elements()
        .find(|w| w.toplevel().unwrap().wl_surface() == surface)
        .cloned()
    {
        if !with_states(surface, |states| {
                states.data_map
                    .get::<XdgToplevelSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .initial_configure_sent
            })
        {
            window.toplevel().unwrap().send_configure();
        }
    }

    popups.commit(surface);
    let Some(popup) = popups.find_popup(surface) else {
        return;
    };

    let PopupKind::Xdg(ref xdg) = popup else {
        return;
    };

    if with_states(surface, |states| {
        states.data_map
            .get::<XdgPopupSurfaceData>()
            .unwrap()
            .lock()
            .unwrap()
            .initial_configure_sent
        })
    {
        return;
    }

    xdg.send_configure()
        .expect("initial configure failed");
}

impl LynWM {
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };

        let Some(window) = self.space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == &root)
        else {
            return;
        };

        let output = self.space.outputs().next().unwrap();
        let output_geometry = self.space.output_geometry(output).unwrap();
        let window_geometry = self.space.element_geometry(window).unwrap();

        let mut target = output_geometry;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geometry.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
