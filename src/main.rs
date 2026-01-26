mod app;
mod renderer;
mod texture;
mod transition;

fn main() -> anyhow::Result<()> {
	app::start()
}
