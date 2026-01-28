use bollard::Docker;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// --- Data Structures ---

#[derive(PartialEq)]
enum Focus {
    FileList,
    Editor,
    Log,
}

struct App {
    project_path: PathBuf,
    status: String,
    logs: Vec<String>,
    files: Vec<PathBuf>,
    file_list_state: ListState,
    current_content: String,
    focus: Focus,
    should_quit: bool,
}

impl App {
    fn new() -> io::Result<App> {
        let path = std::env::current_dir()?;
        let mut app = App {
            project_path: path.clone(),
            status: "OFFLINE".to_string(),
            logs: vec!["[System] Manager started.".into()],
            files: vec![],
            file_list_state: ListState::default(),
            current_content: String::new(),
            focus: Focus::FileList,
            should_quit: false,
        };
        app.refresh_files();
        Ok(app)
    }

    fn refresh_files(&mut self) {
        self.files = WalkDir::new(&self.project_path)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md" || ext == "mdx"))
            .map(|e| e.path().to_path_buf())
            .collect();
        if !self.files.is_empty() && self.file_list_state.selected().is_none() {
            self.file_list_state.select(Some(0));
        }
    }

    fn load_selected_file(&mut self) {
        if let Some(i) = self.file_list_state.selected() {
            if let Ok(content) = fs::read_to_string(&self.files[i]) {
                self.current_content = content;
                self.logs.push(format!("[File] Loaded {}", self.files[i].display()));
            }
        }
    }

    fn save_current_file(&mut self) {
        if let Some(i) = self.file_list_state.selected() {
            if fs::write(&self.files[i], &self.current_content).is_ok() {
                self.logs.push(format!("[File] Saved {}", self.files[i].display()));
            }
        }
    }

    fn git_status(&mut self) {
        let output = std::process::Command::new("git")
            .arg("status")
            .arg("--short")
            .output();
        
        if let Ok(out) = output {
            let status = String::from_utf8_lossy(&out.stdout);
            self.logs.push("-- Git Status --".to_string());
            for line in status.lines() {
                self.logs.push(line.to_string());
            }
        }
    }
}

// --- Main Loop ---

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;

    while !app.should_quit {
        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => app.should_quit = true,
                        KeyCode::Tab => {
                            app.focus = match app.focus {
                                Focus::FileList => Focus::Editor,
                                Focus::Editor => Focus::Log,
                                Focus::Log => Focus::FileList,
                            };
                        }
                        // ファイルナビゲーション
                        KeyCode::Up if app.focus == Focus::FileList => {
                            let i = app.file_list_state.selected().unwrap_or(0);
                            app.file_list_state.select(Some(i.saturating_sub(1)));
                            app.load_selected_file();
                        }
                        KeyCode::Down if app.focus == Focus::FileList => {
                            let i = app.file_list_state.selected().unwrap_or(0);
                            app.file_list_state.select(Some((i + 1).min(app.files.len() - 1)));
                            app.load_selected_file();
                        }
                        // Git操作ショートカット
                        KeyCode::Char('g') => app.git_status(),
                        // 保存ショートカット
                        KeyCode::Char('s') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                            app.save_current_file();
                        }
                        // Docker起動 (Dummy logic for example)
                        KeyCode::Char('r') => {
                            app.status = "RUNNING".to_string();
                            app.logs.push("[Docker] Container started on port 3000".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

// --- UI Rendering ---

fn ui(f: &mut Frame, app: &mut App) {
    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Middle (Files + Editor)
            Constraint::Length(8), // Footer (Logs)
            Constraint::Length(1), // Help Bar
        ])
        .split(size);

    // 1. Header
    let status_color = if app.status == "RUNNING" { Color::Green } else { Color::Yellow };
    let header = Paragraph::new(format!(" Docusaurus Manager | Status: {} | Path: {}", app.status, app.project_path.display()))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(status_color)))
        .style(Style::default().fg(Color::White).bold());
    f.render_widget(header, chunks[0]);

    // 2. Middle Area (Horizontal)
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[1]);

    // File List
    let items: Vec<ListItem> = app.files
        .iter()
        .map(|p| {
            let filename = p.file_name().unwrap().to_string_lossy();
            ListItem::new(format!(" 📄 {}", filename))
        })
        .collect();
    
    let list_block = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Files (Up/Down) "))
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol(">> ");
    
    f.render_stateful_widget(list_block, body_chunks[0], &mut app.file_list_state);

    // Editor Area
    let editor_title = if app.focus == Focus::Editor { " Editor (Editing Mode) " } else { " Editor " };
    let editor_block = Paragraph::new(app.current_content.as_str())
        .block(Block::default().borders(Borders::ALL).title(editor_title)
        .border_style(if app.focus == Focus::Editor { Style::default().fg(Color::Cyan) } else { Style::default() }));
    f.render_widget(editor_block, body_chunks[1]);

    // 3. Logs
    let log_items: Vec<ListItem> = app.logs.iter().rev().take(10).map(|l| ListItem::new(l.as_str())).collect();
    let logs = List::new(log_items).block(Block::default().borders(Borders::ALL).title(" Console Output "));
    f.render_widget(logs, chunks[2]);

    // 4. Help Bar
    let help_menu = Paragraph::new(" [q]Quit | [Tab]Switch Focus | [r]Run Docker | [g]Git Status | [Ctrl+s]Save ");
    f.render_widget(help_menu, chunks[3]);
}
