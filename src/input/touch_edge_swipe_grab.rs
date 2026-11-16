use std::time::Duration;

use smithay::input::touch::{
    DownEvent, GrabStartData as TouchGrabStartData, MotionEvent, OrientationEvent, ShapeEvent,
    TouchGrab, TouchInnerHandle, UpEvent,
};
use smithay::input::SeatHandler;
use smithay::output::Output;
use smithay::utils::{Logical, Point};

use crate::niri::State;

// Threshold copied from libadwaita.
const RECOGNITION_THRESHOLD: f64 = 16.;

pub struct TouchEdgeSwipeGrab {
    start_data: TouchGrabStartData<State>,
    output: Output,
    gesture: GestureState,
    last_location: Point<f64, Logical>,

    // Accumulated and applied in frame().
    new_location: Point<f64, Logical>,
    event_timestamp: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GestureState {
    Recognizing,
    Active { overview: bool, view_offset: bool },
    Passthrough,
}

impl TouchEdgeSwipeGrab {
    pub fn new(start_data: TouchGrabStartData<State>, output: Output) -> Self {
        let location = start_data.location;
        Self {
            start_data,
            output,
            gesture: GestureState::Recognizing,
            last_location: location,
            new_location: location,
            event_timestamp: None,
        }
    }

    fn is_active(&self) -> bool {
        matches!(self.gesture, GestureState::Active { .. })
    }

    fn on_frame(&mut self, data: &mut State, handle: &mut TouchInnerHandle<'_, State>) {
        let Some(timestamp) = self.event_timestamp.take() else {
            return;
        };

        if self.gesture == GestureState::Recognizing {
            let c = self.new_location - self.start_data.location;
            if c.x * c.x + c.y * c.y < RECOGNITION_THRESHOLD * RECOGNITION_THRESHOLD {
                return;
            }

            // Leave swipes going mostly down to the client.
            if c.y > c.x.abs() {
                self.gesture = GestureState::Passthrough;
                return;
            }

            // Take the touch away from the client.
            handle.cancel(data);

            let layout = &mut data.niri.layout;
            layout.overview_gesture_begin();
            layout.view_offset_gesture_begin(&self.output, None, false);
            self.gesture = GestureState::Active {
                overview: true,
                view_offset: true,
            };
            // Don't lose the movement made during recognition.
            self.last_location = self.start_data.location;
        }

        let GestureState::Active {
            overview,
            view_offset,
        } = &mut self.gesture
        else {
            return;
        };

        let layout = &mut data.niri.layout;
        let delta = self.new_location - self.last_location;
        self.last_location = self.new_location;

        let mut redraw = false;

        // Update the overview first since the view offset delta gets divided by the current zoom.
        if *overview {
            match layout.overview_gesture_update(-delta.y, timestamp) {
                Some(r) => redraw |= r,
                None => *overview = false,
            }
        }

        if *view_offset {
            match layout.view_offset_gesture_update(-delta.x, timestamp, false) {
                Some(output) => redraw |= output.is_some(),
                None => *view_offset = false,
            }
        }

        if redraw {
            data.niri.queue_redraw_all();
        }
    }

    fn on_ungrab(&mut self, data: &mut State) {
        let GestureState::Active {
            overview,
            view_offset,
        } = self.gesture
        else {
            return;
        };

        let layout = &mut data.niri.layout;
        if view_offset {
            layout.view_offset_gesture_end(Some(false));
        }
        if overview {
            layout.overview_gesture_end();
        }
        data.niri.queue_redraw_all();
    }
}

impl TouchGrab<State> for TouchEdgeSwipeGrab {
    fn down(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        focus: Option<(<State as SeatHandler>::TouchFocus, Point<f64, Logical>)>,
        event: &DownEvent,
    ) {
        if !self.is_active() {
            handle.down(data, focus, event);
        }
    }

    fn up(&mut self, data: &mut State, handle: &mut TouchInnerHandle<'_, State>, event: &UpEvent) {
        if !self.is_active() {
            handle.up(data, event);
        }

        if event.slot == self.start_data.slot {
            handle.unset_grab(self, data);
        }
    }

    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        focus: Option<(<State as SeatHandler>::TouchFocus, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        if !self.is_active() {
            handle.motion(data, focus, event);
        }

        if event.slot == self.start_data.slot {
            self.new_location = event.location;
            self.event_timestamp = Some(Duration::from_micros(event.time.micros()));
        }
    }

    fn frame(&mut self, data: &mut State, handle: &mut TouchInnerHandle<'_, State>) {
        if !self.is_active() {
            handle.frame(data);
        }
        self.on_frame(data, handle);
    }

    fn cancel(&mut self, data: &mut State, handle: &mut TouchInnerHandle<'_, State>) {
        handle.cancel(data);
        handle.unset_grab(self, data);
    }

    fn shape(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        event: &ShapeEvent,
    ) {
        if !self.is_active() {
            handle.shape(data, event);
        }
    }

    fn orientation(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        event: &OrientationEvent,
    ) {
        if !self.is_active() {
            handle.orientation(data, event);
        }
    }

    fn start_data(&self) -> &TouchGrabStartData<State> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut State) {
        self.on_ungrab(data);
    }
}
