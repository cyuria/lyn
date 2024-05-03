
use smithay::{
    backend::input::{
        AbsolutePositionEvent,
        Axis,
        AxisSource,
        ButtonState,
        Event,
        InputBackend,
        InputEvent,
        KeyboardKeyEvent,
        PointerAxisEvent,
        PointerButtonEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{
            AxisFrame,
            ButtonEvent,
            MotionEvent,
        },
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
};

use crate::LynWM;

impl LynWM {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

                self.seat.get_keyboard().unwrap().input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, _, _| FilterResult::Forward,
                );
            }
            InputEvent::PointerMotion { .. } => {}
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();
                let geometry = self.space.output_geometry(output).unwrap();
                let pos = event.position_transformed(geometry.size) + geometry.loc.to_f64();
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.seat.get_pointer().unwrap();
                let under = self.surface_under(pos);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();
                let serial = SERIAL_COUNTER.next_serial();
                let button = event.button_code();
                let state = event.state();

                if state == ButtonState::Pressed && !pointer.is_grabbed() {
                    let window = match self.space
                        .element_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        Some((window, _loc)) => {
                            self.space.raise_element(&window, true);
                            Some(window.toplevel().unwrap().wl_surface().clone())
                        },
                        None => {
                            self.space.elements().for_each(|window| {
                                window.set_activated(false);
                            });
                            Option::<WlSurface>::None
                        },
                    };
                    self.space.elements().for_each(|window| {
                        window.toplevel().unwrap().send_pending_configure();
                    });
                    keyboard.set_focus(self, window, serial);
                }

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();
                let horizontal_discreet = event.amount_v120(Axis::Horizontal);
                let vertical_discreet = event.amount_v120(Axis::Vertical);
                let horizontal = event
                    .amount(Axis::Horizontal)
                    .unwrap_or(horizontal_discreet.unwrap_or(0.0) * 15.0 / 120.0);
                let vertical = event
                    .amount(Axis::Vertical)
                    .unwrap_or(vertical_discreet.unwrap_or(0.0) * 15.0 / 120.0);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal);
                    if let Some(discreet) = horizontal_discreet {
                        frame = frame.v120(Axis::Horizontal, discreet as i32);
                    }
                }
                if vertical != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical);
                    if let Some(discreet) = vertical_discreet {
                        frame = frame.v120(Axis::Vertical, discreet as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}

