
mod compositor;
mod xdg_shell;

use crate::LynWM;

use smithay::{
    delegate_data_device,
    delegate_output,
    delegate_seat,
    input::{
        Seat,
        SeatHandler,
        SeatState,
    },
    reexports::wayland_server::{
        protocol::wl_surface::WlSurface,
        Resource,
    },
    wayland::{
        output::OutputHandler,
        selection::{
            data_device::{
                set_data_device_focus,
                ClientDndGrabHandler,
                DataDeviceHandler,
                DataDeviceState,
                ServerDndGrabHandler,
            },
            SelectionHandler,
        },
    },
};

impl SeatHandler for LynWM {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: smithay::input::pointer::CursorImageStatus) {}

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&Self::KeyboardFocus>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}

delegate_seat!(LynWM);

impl SelectionHandler for LynWM {
    type SelectionUserData = ();
}

impl DataDeviceHandler for LynWM {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for LynWM {}
impl ServerDndGrabHandler for LynWM {}

delegate_data_device!(LynWM);

impl OutputHandler for LynWM {}
delegate_output!(LynWM);

