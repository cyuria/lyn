
use std::{ ffi::OsString, sync::Arc };

use smithay::{
    desktop::{
        PopupManager,
        Space,
        Window,
        WindowSurfaceType,
    },
    input::{
        Seat,
        SeatState,
    },
    reexports::{
        calloop::{
            generic::Generic,
            EventLoop,
            Interest,
            LoopSignal,
            Mode,
            PostAction,
        },
        wayland_server::{
            backend::{
                ClientData,
                ClientId,
                DisconnectReason,
            },
            protocol::wl_surface::WlSurface,
            Display,
            DisplayHandle,
        },
    },
    utils::{
        Logical,
        Point,
    },
    wayland::{
        compositor::{
            CompositorClientState,
            CompositorState,
        },
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::CalloopData;

pub struct LynWM {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<LynWM>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub seat: Seat<Self>,
}

impl LynWM {
    pub fn new(event_loop: &mut EventLoop<CalloopData>, display: Display<Self>) -> Self {
        let dh = display.handle();

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "winit");
        seat.add_keyboard(Default::default(), 200, 25).unwrap();
        seat.add_pointer();

        Self {
            start_time: std::time::Instant::now(),

            space: Space::default(),
            loop_signal: event_loop.get_signal(),
            socket_name: Self::init_wayland_listener(display, event_loop),

            compositor_state: CompositorState::new::<Self>(&dh),
            xdg_shell_state: XdgShellState::new::<Self>(&dh),
            shm_state: ShmState::new::<Self>(&dh, vec![]),
            output_manager_state: OutputManagerState::new_with_xdg_output::<Self>(&dh),
            seat_state,
            data_device_state: DataDeviceState::new::<Self>(&dh),
            popups: PopupManager::default(),
            seat,

            display_handle: dh,
        }
    }

    fn init_wayland_listener(
        display: Display<LynWM>,
        event_loop: &mut EventLoop<CalloopData>,
    ) -> OsString {
        let socket = ListeningSocketSource::new_auto().unwrap();
        let name = socket.socket_name().to_os_string();
        let handle = event_loop.handle();

        handle
            .insert_source(socket, move |stream, _, state| {
                state.display_handle
                    .insert_client(stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to initialise the wayland event source.");

        handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    /* Safety: avoid dropping display */
                    unsafe {
                        display
                            .get_mut()
                            .dispatch_clients(&mut state.state)
                            .unwrap();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        name
    }

    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<i32, Logical>)> {
        self.space.element_under(pos).and_then(|(window, location)| {
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(s, p)| (s, p + location))
        })
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
