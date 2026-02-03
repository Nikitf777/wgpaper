use keyframe::{AnimationSequence, functions::BezierCurve, keyframes, mint::Vector2};
use std::time::Duration;

pub struct TransitionProgress {
	pub progress_bezier: f32,
	pub progress_linear: f32,
}

impl TransitionProgress {
	pub fn reset() -> Self {
		Self {
			progress_bezier: 0.0,
			progress_linear: 0.0,
		}
	}
	pub fn finished() -> Self {
		Self {
			progress_bezier: 1.0,
			progress_linear: 1.0,
		}
	}
	pub fn is_finished(&self) -> bool {
		self.progress_bezier == 1.0 && self.progress_linear >= 1.0
	}
}

pub struct Transition {
	sequence: AnimationSequence<f32>,
}

impl Transition {
	pub fn new(duration: f32, bezier: (f32, f32, f32, f32)) -> Transition {
		let bezier = BezierCurve::from(
			Vector2 {
				x: bezier.0,
				y: bezier.1,
			},
			Vector2 {
				x: bezier.2,
				y: bezier.3,
			},
		);
		Self {
			sequence: keyframes![(0.0, 0.0, bezier), (1.0, duration, bezier)],
		}
	}

	pub fn advance_to(&mut self, timestamp: Duration) -> TransitionProgress {
		let transition_progress_clamped = timestamp.as_secs_f32() / self.sequence.duration() as f32;
		self.sequence.advance_to(timestamp.as_secs_f64());
		TransitionProgress {
			progress_bezier: self.sequence.now(),
			progress_linear: transition_progress_clamped,
		}
	}
}

impl Default for Transition {
	fn default() -> Self {
		Self::new(1.0, (0.54, 0.0, 0.34, 0.99))
	}
}
