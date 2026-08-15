use log::info;

mod ai;
mod app;
mod canvas;
mod compiler;
mod editor;
mod plugins;
mod preview;
mod project;
mod ui;
mod workspace;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    info!("graf starting");

    app::run();
}
