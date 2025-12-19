use crate::app::{App, Mode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Row, Table, Wrap},
    Frame,
};
use throbber_widgets_tui::Throbber;

pub fn ui(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // Help popup
    if app.show_help {
        render_help_popup(f, app, size);
        return;
    }

    match app.mode {
        Mode::Insert | Mode::Normal => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header (Boxed)
                    Constraint::Min(1),    // History
                    Constraint::Length(
                        (3 + app.input.lines().len().saturating_sub(1) as u16).min(10),
                    ), // Input grows to max 10 lines
                    Constraint::Length(1), // StatusBar
                ])
                .split(size);

            render_header(f, app, chunks[0]);
            render_chat_history(f, app, chunks[1]);
            render_input(f, app, chunks[2]);
            render_status_bar(f, app, chunks[3]);
        }
        Mode::ModelSelect => {
            render_model_select(f, app, size);
        }
        Mode::SystemPromptEdit => {
            // Background dimming could be nice here if we had an overlay widget
            render_system_prompt_edit(f, app, size);
        }
        Mode::SessionSelect => {
            render_session_select(f, app, size);
        }
        Mode::SessionCreate => {
            render_session_create(f, app, size);
        }
        Mode::ModelPullInput => {
            render_model_pull_input(f, app, size);
        }
        Mode::ToolConfirmation => {
            render_tool_confirmation(f, app, size);
        }
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let model_name = app
        .models
        .get(app.selected_model)
        .map(|s| s.as_str())
        .unwrap_or("No Model");

    let title_text = format!(" Ollama TUI | Model: {} ", model_name);
    
    let header_style = Style::default()
        .fg(app.theme.header_fg)
        .bg(app.theme.header_bg)
        .add_modifier(Modifier::BOLD);

    let p = Paragraph::new(title_text)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(app.theme.header_border)).border_type(BorderType::Rounded))
        .style(header_style)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(p, area);
}

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let mode_str = match app.mode {
        Mode::Insert => "INSERT",
        Mode::Normal => "NORMAL",
        _ => "MENU",
    };

    let status_text = if let Some((msg, time)) = &app.notification {
        if time.elapsed().as_secs() < 3 {
             format!(" {}", msg)
        } else {
             format!(
                " {} | Session: {} | Tokens: {}/{} | F1: Help ",
                mode_str, app.current_session, app.current_token_usage, app.context_token_limit
            )
        }
    } else if let Some((status, completed, total)) = &app.pull_progress {
         let percent = if let (Some(c), Some(t)) = (completed, total) {
             if *t > 0 { (*c as f64 / *t as f64 * 100.0) as u16 } else { 0 }
         } else { 0 };
         format!(" Pulling: {} ({}%) ", status, percent)
    } else {
        let limit = app.model_context_limit.unwrap_or(app.context_token_limit);
        format!(
            " {} | Session: {} | Tokens: {}/{} | F1: Help ",
            mode_str, app.current_session, app.current_token_usage, limit
        )
    };

    let style = app.theme.status_bar();
    let p = Paragraph::new(status_text)
        .style(style)
        .alignment(ratatui::layout::Alignment::Left);
    f.render_widget(p, area);
}

fn render_chat_history(f: &mut Frame, app: &mut App, area: Rect) {
    let history_area = area;
    let width = history_area.width;
    let max_available_width = (width as f32 * 0.90) as u16;

    if app.messages.is_empty() {
        let empty_text = "Start a conversation.\nCtrl+o: Model | Ctrl+r: Sessions | Ctrl+s: System Prompt";
        let p = Paragraph::new(empty_text)
            .alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().fg(app.theme.secondary_fg));
        let area = centered_rect(80, 50, history_area);
        f.render_widget(p, area);
    }

    // 1. Calculate layouts
    // Tuple: (height, Option<Text>, is_thinking, bubble_width)
    let mut calculated_msgs: Vec<(u16, Option<Text>, bool, u16)> = Vec::new();
    let mut total_height: u16 = 0;

    let msg_count = app.messages.len();
    for (i, msg) in app.messages.iter().enumerate() {
        let is_thinking = app.loading
            && i == msg_count - 1
            && msg.role == "assistant"
            && msg.content.trim().is_empty();

        if is_thinking {
            let height = 3;
            // Thinking bubble fixed width or dynamic? Let's make it fixed wide enough for "Thinking..."
            let bubble_width = 16.min(max_available_width); 
            calculated_msgs.push((height, None, true, bubble_width));
            total_height += height;
        } else {
            let content_to_render = if msg.tool_name.is_some() {
                 insert_soft_hyphens(&msg.content)
            } else {
                msg.content.clone()
            };

            let md_borrowed = tui_markdown::from_str(&content_to_render);
            let md = to_owned_text(md_borrowed);
            
            // Calculate content width
            // We want the bubble to fit the content, up to max_available_width.
            // Minimum width of e.g. 10 chars to avoid tiny bubbles.
            let content_text_width = md.lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16;
            
            // Add padding (2 for borders, maybe 2 for internal padding)
            // Let's assume Borders take 2.
            let required_width = content_text_width.saturating_add(4); // +4 for safe padding inside borders
            
            let bubble_width = required_width.clamp(14, max_available_width);
            let wrapping_width = bubble_width.saturating_sub(4); // Inner width for text wrapping

            let height = estimate_wrapped_height(&md, wrapping_width) + 2; // +2 for border height

            calculated_msgs.push((height, Some(md), false, bubble_width));
            total_height += height;
        }
    }

    // Add margins (1 line between bubbles)
    if !calculated_msgs.is_empty() {
        total_height += (calculated_msgs.len() as u16).saturating_sub(1);
    }

    // Scroll Logic
    let viewport_height = history_area.height;
    if app.auto_scroll {
        if total_height > viewport_height {
            app.vertical_scroll = total_height - viewport_height;
        } else {
            app.vertical_scroll = 0;
        }
    } else {
        let max_scroll = total_height.saturating_sub(viewport_height);
        if app.vertical_scroll > max_scroll {
            app.vertical_scroll = max_scroll;
        }
    }

    // Ensure selected message is visible
    if let Some(selected_idx) = app.selected_message_index {
         let mut current_y_offset = 0;
         for (i, (height, _, _, _)) in calculated_msgs.iter().enumerate() {
              if i == selected_idx {
                  let msg_top = current_y_offset;
                  let msg_bottom = current_y_offset + height;
                  let viewport_h = history_area.height;
                  
                  if msg_top < app.vertical_scroll {
                       app.vertical_scroll = msg_top;
                  } else if msg_bottom > app.vertical_scroll + viewport_h {
                       if *height > viewport_h {
                            app.vertical_scroll = msg_top;
                       } else {
                            app.vertical_scroll = msg_bottom.saturating_sub(viewport_h);
                       }
                  }
                  break;
              }
              current_y_offset += height + 1; // +1 for margin
         }
    }

    // Render Visible Bubbles
    let mut current_y = -(app.vertical_scroll as i32);

    for (i, (height, text_opt, is_thinking, bubble_width)) in calculated_msgs.into_iter().enumerate() {
        let msg_role = &app.messages[i].role;
        let is_user = msg_role == "user";
        let is_tool = app.messages[i].tool_name.is_some();
        let bubble_height = height;

        if current_y + (bubble_height as i32) > 0 && current_y < (viewport_height as i32) {
            let x = if is_user {
                width.saturating_sub(bubble_width)
            } else {
                0
            };

            let area_top = history_area.y;
            let area_bottom = history_area.bottom();

            let item_top = (area_top as i32 + current_y) as i32;
            let item_bottom = item_top + bubble_height as i32;

            let visible_top = item_top.max(area_top as i32);
            let visible_bottom = item_bottom.min(area_bottom as i32);

            if visible_bottom > visible_top {
                let visible_height = (visible_bottom - visible_top) as u16;
                let visible_y = visible_top as u16;

                let rect = Rect::new(history_area.x + x, visible_y, bubble_width, visible_height);

                // Styling logic
                let (border_color, title) = if is_user {
                    (app.theme.user_bubble_border, " You ".to_string())
                } else if is_tool {
                      (app.theme.tool_bubble_fg, format!(" Tool Output: {} ", app.messages[i].tool_name.as_deref().unwrap_or("Unknown")))
                } else {
                    (app.theme.ai_bubble_border, " AI ".to_string())
                };

                let border_style = if Some(i) == app.selected_message_index {
                      Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                      Style::default().fg(border_color)
                };


                // If block borders excludes TOP, title isn't rendered. 
                // Let's render title manually or use `Borders::LEFT`.
                // Actually, let's keep Borders::ALL but make it `BorderType::Rounded` with the DIM color 
                // OR use a very minimal style.
                // Reverting to Borders::ALL but with theme colors is safer for "Clean" vs "Broken" look.
                // Creating a custom "clean" look without borders requires manually drawing the header line.
                // Let's try Borders::LEFT | Borders::TOP to support title.
                
                let clean_block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border_style)
                    .title(title);
                
                // We need to render the Header Title manually if we don't have top border
                // Or just assume the user knows Left=User, Right=AI?
                // Let's put a small header span inside the paragraph.

                if is_thinking {
                    let throbber = Throbber::default().label("Thinking...").throbber_style(
                        Style::default()
                            .fg(Color::LightCyan)
                            .add_modifier(Modifier::BOLD),
                    );
                    let thinking_block = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(app.theme.ai_bubble_border))
                        .title(" AI ");
                    
                    f.render_widget(thinking_block.clone(), rect);
                     let inner_area = thinking_block.inner(rect);
                    f.render_stateful_widget(throbber, inner_area, &mut app.spinner_state);
                } else if let Some(text) = text_opt {
                     // We need to inject the "Title" into the text or render it separately.
                     // A simple way is to use a block structure that has a top margin?
                     // Let's just use the clean block and maybe prepend "You:" or "AI:" to the text?
                     // `tui_markdown` returns `Text`.
                     
                     // Let's render the Block with Left Border.
                     f.render_widget(clean_block.clone(), rect);
                     let inner_area = clean_block.inner(rect);
                     
                     // Manually render a "Title" line?
                     // Or just rely on the content. The bubbles are left/right aligned, providing context.
                     // Let's stick to functional cleanliness.
                     
                     let display_text = if is_tool {
                         text.clone() 
                     } else {
                         text.clone() 
                     };

                     let scroll_offset = if item_top < area_top as i32 {
                         (area_top as i32 - item_top) as u16
                     } else {
                         0
                     };
                     
                     let alignment = if is_user {
                         ratatui::layout::Alignment::Right
                     } else {
                         ratatui::layout::Alignment::Left
                     };

                     let p = Paragraph::new(display_text)
                         .wrap(Wrap { trim: false })
                         .alignment(alignment)
                         .scroll((scroll_offset, 0));
                     f.render_widget(p, inner_area);
                }
            }
        }
        current_y += bubble_height as i32 + 1;
    }
    
    // "More Content" Indicator
    let max_scroll = total_height.saturating_sub(viewport_height);
    if app.vertical_scroll < max_scroll {
         let indicator_area = Rect::new(history_area.x, history_area.bottom() - 1, width, 1);
         f.render_widget(Clear, indicator_area); 
         let indicator = Paragraph::new("v More v")
            .alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().fg(app.theme.secondary_fg).add_modifier(Modifier::DIM));
         f.render_widget(indicator, indicator_area);
    }
}

fn render_input(f: &mut Frame, app: &mut App, area: Rect) {
    let (input_border_color, _input_title) = if let Some(err) = &app.error {
        (app.theme.input_border_error, format!(" Error: {} ", err))
    } else {
        match app.mode {
            Mode::Insert => (
                app.theme.input_border_active,
                " Input (Insert Mode) ".to_string(),
            ),
            Mode::Normal => (app.theme.input_border_normal, " Input (Normal Mode) ".to_string()),
            _ => (app.theme.input_border_normal, " Input ".to_string()),
        }
    };

    match app.mode {
        Mode::Insert => app.input.set_style(Style::default().fg(app.theme.primary_fg)),
        _ => app
            .input
            .set_style(Style::default().fg(app.theme.secondary_fg).add_modifier(Modifier::DIM)),
    }

    app.input.set_block(
        Block::default()
            .borders(Borders::TOP) // Only top border for cleaner Input look? Or rounded all?
            // Let's keep Rounded ALL for Input to make it look like a text field.
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(input_border_color)),
    );
    f.render_widget(&app.input, area);
}

fn render_help_popup(f: &mut Frame, app: &App, size: Rect) {
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(app.theme.modal_border));
        
    let area = centered_rect(60, 65, size);
    f.render_widget(Clear, area);
    
    // Table content
    let rows = vec![
        Row::new(vec!["General", ""]),
        Row::new(vec![" Ctrl+o", "Model Select"]),
        Row::new(vec![" Ctrl+r", "Session Manager"]),
        Row::new(vec![" Ctrl+s", "System Prompt"]),
        Row::new(vec![" Ctrl+l", "Clear History"]),
        Row::new(vec![" F1", "Toggle Help"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["Insert Mode", ""]),
        Row::new(vec![" Enter", "Send Message"]),
        Row::new(vec![" Shift+Enter", "New Line"]),
        Row::new(vec![" Esc", "Normal Mode"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["Normal Mode", ""]),
        Row::new(vec![" j/k", "Scroll"]),
        Row::new(vec![" i", "Switch to Insert"]),
        Row::new(vec![" q", "Quit"]),
    ];
    
    let table = Table::new(rows, [Constraint::Percentage(30), Constraint::Percentage(70)])
        .block(block)
        .header(Row::new(vec!["Key", "Action"]).style(Style::default().add_modifier(Modifier::BOLD).fg(app.theme.primary_fg)));
        
    f.render_widget(table, area);
}

// ... (Other render functions calling reuse components or similar simple blocks)
// For brevity, I will apply simple styles to them using theme.

fn render_model_select(f: &mut Frame, app: &mut App, size: Rect) {
    let area = centered_rect(60, 40, size);
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Select Model (p: Pull, d: Delete) ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.modal_border));
        
    let items: Vec<ListItem> = app
        .models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let s = if i == app.selected_model {
                Style::default().fg(app.theme.primary_fg).add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(app.theme.secondary_fg)
            };
            ListItem::new(Span::styled(m, s))
        })
        .collect();
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn render_session_select(f: &mut Frame, app: &mut App, size: Rect) {
    let area = centered_rect(60, 50, size);
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Session Manager (c: Create, d: Delete, Enter: Select) ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.modal_border));

    let items: Vec<ListItem> = app
        .available_sessions
        .iter()
        .map(|s| {
            let style = if s == &app.current_session {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.secondary_fg)
            };
            // Add indicator text
            let text = if s == &app.current_session {
                 format!("{} (current)", s)
            } else {
                 s.clone()
            };
            ListItem::new(Span::styled(text, style))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    f.render_stateful_widget(list, area, &mut app.session_list_state);
}

fn render_system_prompt_edit(f: &mut Frame, app: &mut App, size: Rect) {
    let area = centered_rect(80, 30, size);
    f.render_widget(Clear, area);

    app.system_prompt_input.set_block(
        Block::default()
            .title(" Edit System Prompt ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.modal_border)),
    );
    f.render_widget(&app.system_prompt_input, area);
}

fn render_session_create(f: &mut Frame, app: &mut App, size: Rect) {
    let area = centered_rect(60, 20, size);
    f.render_widget(Clear, area);

    app.session_input.set_block(
        Block::default()
            .title(" Create New Session (Enter Name) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green)),
    );
    f.render_widget(&app.session_input, area);
}

fn render_model_pull_input(f: &mut Frame, app: &mut App, size: Rect) {
    let area = centered_rect(60, 20, size);
    f.render_widget(Clear, area);
    app.pull_input.set_block(
        Block::default()
            .title(" Pull Model (Enter Name) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(&app.pull_input, area);
}

fn render_tool_confirmation(f: &mut Frame, app: &mut App, size: Rect) {
    let area = centered_rect(60, 40, size);
    f.render_widget(Clear, area);

    if let Some(tool_call) = &app.pending_tool_call {
        let tool_name = &tool_call.function.name;
        let args_str = serde_json::to_string_pretty(&tool_call.function.arguments)
            .unwrap_or_else(|_| "Invalid JSON".to_string());

        let text = format!("Tool: {}\n\nArguments:\n{}\n\nAllow execution? (y/n)", tool_name, args_str);
        
        let p = Paragraph::new(text)
            .block(Block::default()
                .title(" Confirm Tool Execution (Scroll with Up/Down or j/k) ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Red)))
            .wrap(Wrap { trim: false })
            .scroll((app.tool_scroll, 0));
        f.render_widget(p, area);
    }
}

// Helpers
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn estimate_wrapped_height(text: &Text, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    let mut height = 0;
    for line in &text.lines {
        let line_width = line.width() as u16;
        if line_width == 0 {
            height += 1;
        } else {
            height += (line_width as f32 / width as f32).ceil() as u16;
        }
    }
    height
}

fn to_owned_text(text: Text) -> Text<'static> {
    let lines: Vec<_> = text.lines.into_iter().map(|line| {
        let spans: Vec<_> = line.spans.into_iter().map(|span| {
            Span::styled(span.content.into_owned(), span.style)
        }).collect();
        ratatui::text::Line::from(spans)
    }).collect();
    Text::from(lines)
}

fn insert_soft_hyphens(text: &str) -> String {
    // \u{200B} is ZERO WIDTH SPACE
    text.replace('/', "/\u{200B}")
        .replace('_', "_\u{200B}")
        .replace(',', ",\u{200B}")
}
