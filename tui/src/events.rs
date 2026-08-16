use crate::app::{App, FilterField, Pane};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    if app.editing {
        handle_editing(app, key);
        return;
    }

    // Alt+arrows switch panes (in addition to Tab / Esc).
    if key.modifiers.contains(KeyModifiers::ALT) {
        match key.code {
            KeyCode::Right | KeyCode::Down => {
                app.cycle_pane();
                return;
            }
            KeyCode::Left | KeyCode::Up => {
                app.cycle_pane_back();
                return;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => {
            // Always allow leaving Downloads / exiting edit mode via Esc.
            if app.focus == Pane::Downloads {
                app.focus = Pane::Results;
            } else if app.focus == Pane::Results {
                app.focus = Pane::Filters;
            }
        }
        KeyCode::Tab => app.cycle_pane(),
        KeyCode::BackTab => app.cycle_pane_back(),
        KeyCode::Char('s') => app.start_search(false),
        KeyCode::Char('S') => app.start_search(true),
        KeyCode::Char(' ') if app.focus == Pane::Results => app.toggle_mark_selected(),
        KeyCode::Char('a' | 'A') if app.focus == Pane::Results => app.start_download_all(),
        KeyCode::Char('d' | 'D') | KeyCode::Enter if app.focus == Pane::Results => {
            app.start_download_selected();
        }
        // Allow starting another download from the Downloads pane via `d`
        // using the current results selection / marks.
        KeyCode::Char('d' | 'D') if app.focus == Pane::Downloads => {
            app.start_download_selected();
        }
        KeyCode::Char('a' | 'A') if app.focus == Pane::Downloads => {
            app.start_download_all();
        }
        other => match app.focus {
            Pane::Filters => handle_filters(app, other, key.modifiers),
            Pane::Results => handle_results(app, other),
            Pane::Downloads => handle_downloads(app, other),
        },
    }
}

fn handle_filters(app: &mut App, code: KeyCode, _modifiers: KeyModifiers) {
    match code {
        KeyCode::Up | KeyCode::Char('k') => app.prev_filter_field(),
        KeyCode::Down | KeyCode::Char('j') => app.next_filter_field(),
        KeyCode::Left | KeyCode::Char('h') => match app.filter_field {
            FilterField::Satellite => app.cycle_satellite(false),
            FilterField::Product => app.cycle_product(false),
            _ => {}
        },
        KeyCode::Right | KeyCode::Char('l') => match app.filter_field {
            FilterField::Satellite => app.cycle_satellite(true),
            FilterField::Product => app.cycle_product(true),
            _ => {}
        },
        KeyCode::Enter | KeyCode::Char('e') => {
            if app.filter_field.is_text() {
                app.editing = true;
            } else {
                match app.filter_field {
                    FilterField::Satellite => app.cycle_satellite(true),
                    FilterField::Product => app.cycle_product(true),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn handle_results(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Up | KeyCode::Char('k') => app.select_prev_result(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next_result(),
        _ => {}
    }
}

fn handle_downloads(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Up | KeyCode::Char('k') => app.select_prev_download(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next_download(),
        _ => {}
    }
}

fn handle_editing(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => {
            app.editing = false;
        }
        KeyCode::Backspace => {
            if let Some(value) = app.field_value_mut(app.filter_field) {
                value.pop();
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(value) = app.field_value_mut(app.filter_field) {
                value.clear();
            }
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(value) = app.field_value_mut(app.filter_field) {
                value.push(c);
            }
        }
        _ => {}
    }
}
