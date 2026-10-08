use keyframe::{AnimationSequence, functions::BezierCurve, keyframes, mint::Vector2};
use std::time::{Duration, Instant};

/// A point in time along a transition animation.
///
/// Re-exported from `wgpaper-abi` rather than declared here: this pair of
/// `f32`s *is* the wire format (`progress_bezier` / `progress_linear` in the
/// shared uniform block), so the type the host computes with and the type the
/// shaders read must be the same one.
pub use wgpaper_abi::TransitionProgress;

/// The easing curve of a transition, with no notion of when it started.
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

	/// Eased and linear progress for a given time since the transition began.
	///
	/// Returns `finished()` once `elapsed` reaches the duration, rather than
	/// relying on the curve landing on exactly `1.0`. Termination is decided by
	/// the clock, so it cannot be missed by a floating-point rounding error —
	/// which matters because `is_finished` compares against `1.0` exactly.
	pub fn progress_at(&mut self, elapsed: Duration) -> TransitionProgress {
		let duration = self.sequence.duration();

		if elapsed.as_secs_f64() >= duration {
			return TransitionProgress::finished();
		}

		// `advance_to` moves the sequence's clock and returns the new time
		// position; the eased value itself comes from `now`.
		self.sequence.advance_to(elapsed.as_secs_f64());
		let progress_bezier = self.sequence.now();
		let progress_linear = elapsed.as_secs_f32() / duration as f32;

		TransitionProgress {
			progress_bezier,
			progress_linear,
		}
	}
}

impl Default for Transition {
	fn default() -> Self {
		Self::new(1.0, (0.54, 0.0, 0.34, 0.99))
	}
}

/// The transition state of a *single* output.
///
/// Each output owns one of these, so an output that is not animating is
/// unaffected by its neighbours, and an output can be started on its own.
/// This replaces a single clock shared by every output and pushed down through
/// `OutputManager::frame`, which meant one output reaching the end of a
/// transition cancelled the animation on all the others.
pub struct ActiveTransition {
	transition: Transition,
	/// `None` when this output is not animating.
	///
	/// The idle state is modelled explicitly rather than as "a clock that
	/// started long ago", so `is_active` is a plain state question.
	start_time: Option<Instant>,
}

impl ActiveTransition {
	pub fn new(duration: f32, bezier: (f32, f32, f32, f32)) -> Self {
		Self {
			transition: Transition::new(duration, bezier),
			start_time: None,
		}
	}

	/// Begin animating.
	pub fn start(&mut self) {
		self.start_at(Instant::now());
	}

	/// Begin animating as though it started at `now`.
	///
	/// The counterpart to [`Self::advance_at`], so a caller (or a test) can
	/// drive the whole state machine against a synthetic clock.
	pub fn start_at(&mut self, now: Instant) {
		self.start_time = Some(now);
	}

	/// Stop animating. The output stays wherever it is.
	pub fn stop(&mut self) {
		self.start_time = None;
	}

	pub fn is_active(&self) -> bool {
		self.start_time.is_some()
	}

	/// Advance to `now` and return the new progress.
	///
	/// Returns `None` when no transition is running. Automatically stops once
	/// the transition completes, so a caller can treat a `Some` followed by
	/// `is_active() == false` as "that was the last frame".
	pub fn advance_at(&mut self, now: Instant) -> Option<TransitionProgress> {
		let start_time = self.start_time?;
		let elapsed = now.saturating_duration_since(start_time);
		let progress = self.transition.progress_at(elapsed);

		if progress.is_finished() {
			self.start_time = None;
		}

		Some(progress)
	}

	/// Advance to the current time. See [`Self::advance_at`].
	pub fn advance(&mut self) -> Option<TransitionProgress> {
		self.advance_at(Instant::now())
	}
}

impl Default for ActiveTransition {
	fn default() -> Self {
		Self {
			transition: Transition::default(),
			start_time: None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A transition is idle until it is explicitly started.
	#[test]
	fn starts_idle() {
		let mut transition = ActiveTransition::default();
		assert!(!transition.is_active());
		assert_eq!(transition.advance(), None);
	}

	#[test]
	fn advance_while_idle_yields_nothing() {
		let mut transition = ActiveTransition::default();
		assert_eq!(transition.advance_at(Instant::now()), None);
		assert!(!transition.is_active());
	}

	#[test]
	fn stop_halts_an_active_transition() {
		let mut transition = ActiveTransition::new(1.0, (0.0, 0.0, 1.0, 1.0));
		transition.start_at(Instant::now());
		assert!(transition.is_active());

		transition.stop();
		assert!(!transition.is_active());
		assert_eq!(transition.advance(), None);
	}

	#[test]
	fn restarting_rebegins_from_zero() {
		let mut transition = ActiveTransition::new(10.0, (0.0, 0.0, 1.0, 1.0));
		let start = Instant::now();

		transition.start_at(start);
		let midway = transition
			.advance_at(start + Duration::from_millis(500))
			.unwrap();
		assert!((midway.progress_linear - 0.05).abs() < 1e-6);

		// Restarting must not resume from the previous position.
		transition.start_at(start);
		let restarted = transition.advance_at(start).unwrap();
		assert_eq!(restarted.progress_bezier, 0.0);
		assert_eq!(restarted.progress_linear, 0.0);
	}

	#[test]
	fn progress_reaches_one_at_the_duration() {
		let mut transition = ActiveTransition::new(1.0, (0.54, 0.0, 0.34, 0.99));
		let start = Instant::now();
		transition.start_at(start);

		let done = transition
			.advance_at(start + Duration::from_secs(1))
			.unwrap();
		assert!(done.is_finished());
	}

	#[test]
	fn a_finished_transition_stops_itself() {
		let mut transition = ActiveTransition::new(0.5, (0.54, 0.0, 0.34, 0.99));
		let start = Instant::now();
		transition.start_at(start);

		let last = transition.advance_at(start + Duration::from_millis(500));
		assert!(last.is_some_and(|progress| progress.is_finished()));

		// The clock has gone idle, so no further frames are produced...
		assert!(!transition.is_active());
		assert_eq!(transition.advance(), None);
	}

	#[test]
	fn advancing_past_the_duration_stays_finished() {
		let mut transition = ActiveTransition::new(0.2, (0.54, 0.0, 0.34, 0.99));
		let start = Instant::now();
		transition.start_at(start);

		// Far beyond the duration the progress must not overshoot past 1.0,
		// which would make a shader sample outside its intended range.
		let late = transition
			.advance_at(start + Duration::from_secs(30))
			.unwrap();
		assert_eq!(late.progress_bezier, 1.0);
		assert_eq!(late.progress_linear, 1.0);
	}

	/// Two transitions that start at different times are fully independent,
	/// which is the whole reason the clock moved down to the output.
	#[test]
	fn transitions_do_not_interfere() {
		let mut early = ActiveTransition::new(1.0, (0.0, 0.0, 1.0, 1.0));
		let mut late = ActiveTransition::new(1.0, (0.0, 0.0, 1.0, 1.0));
		let start = Instant::now();

		early.start_at(start);
		late.start_at(start + Duration::from_millis(500));

		// The first finishes while the second is only halfway.
		assert!(
			early
				.advance_at(start + Duration::from_secs(1))
				.unwrap()
				.is_finished()
		);

		let still_running = late.advance_at(start + Duration::from_secs(1)).unwrap();
		assert!((still_running.progress_linear - 0.5).abs() < 1e-6);
		assert!(late.is_active());
	}

	#[test]
	fn a_zero_length_transition_completes_immediately() {
		let mut transition = ActiveTransition::new(0.0, (0.54, 0.0, 0.34, 0.99));
		transition.start();

		let progress = transition.advance().expect("just started");
		assert!(progress.is_finished());
		assert!(!transition.is_active());
	}

	#[test]
	fn linear_progress_tracks_elapsed_time() {
		let mut transition = Transition::new(2.0, (0.0, 0.0, 1.0, 1.0));

		let halfway = transition.progress_at(Duration::from_secs(1));
		assert!((halfway.progress_linear - 0.5).abs() < 1e-6);

		let quarter = transition.progress_at(Duration::from_millis(500));
		assert!((quarter.progress_linear - 0.25).abs() < 1e-6);
	}
}
