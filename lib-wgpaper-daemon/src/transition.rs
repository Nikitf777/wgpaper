use keyframe::{AnimationSequence, functions::BezierCurve, keyframes, mint::Vector2};
use std::time::{Duration, Instant};

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
	pub fn new(duration: f32, bezier: (f32, f32, f32, f32)) -> Self {
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

	pub fn duration(&self) -> f64 {
		self.sequence.duration()
	}

	pub fn advance_to(&mut self, timestamp: Duration) -> f32 {
		self.sequence.advance_to(timestamp.as_secs_f64());
		self.sequence.now()
	}
}

impl Default for Transition {
	fn default() -> Self {
		Self::new(1.0, (0.54, 0.0, 0.34, 0.99))
	}
}

pub struct ActiveTransition {
	transition: Transition,
	start_time: Instant,
}

impl ActiveTransition {
	pub fn new(duration: f32, bezier: (f32, f32, f32, f32)) -> Self {
		Self {
			transition: Transition::new(duration, bezier),
			start_time: Instant::now(),
		}
	}

	pub fn start(&mut self) {
		self.start_time = Instant::now();
	}

	pub fn progress(&mut self) -> TransitionProgress {
		let timestamp = Instant::now().duration_since(self.start_time);
		let progress_linear = timestamp.as_secs_f32() / self.transition.duration() as f32;
		TransitionProgress {
			progress_bezier: self.transition.advance_to(timestamp),
			progress_linear,
		}
	}
}

impl Default for ActiveTransition {
	fn default() -> Self {
		Self {
			transition: Transition::default(),
			start_time: Instant::now(),
		}
	}
}
