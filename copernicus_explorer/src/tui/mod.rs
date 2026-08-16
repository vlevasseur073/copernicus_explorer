//! Interactive terminal UI for Copernicus Explorer (ratatui / crossterm).

mod app;
mod events;
mod ui;

use app::App;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::time::Duration;

/// Run the interactive TUI until the user quits.
pub fn run() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let mut app = App::new();

    let result = run_app(&mut terminal, &mut app);

    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        app.poll_messages();
        terminal.draw(|frame| ui::draw(frame, app))?;

        while event::poll(Duration::from_millis(0))? {
            handle_event(app, event::read()?)?;
        }

        if event::poll(Duration::from_millis(50))? {
            handle_event(app, event::read()?)?;
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn handle_event(app: &mut App, event: Event) -> io::Result<()> {
    if let Event::Key(key) = event
        && key.kind != KeyEventKind::Release
    {
        events::handle_key(app, key);
    }
    Ok(())
}
