pub fn lerp_vec2f(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
	(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

pub trait Lerp {
	fn lerp(&self, other: (f32, f32), t: f32) -> Self;
}

impl Lerp for (f32, f32) {
	fn lerp(&self, other: (f32, f32), t: f32) -> Self {
		lerp_vec2f((self.0, self.1), (other.0, other.1), t)
	}
}
