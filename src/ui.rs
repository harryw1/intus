use crate::app::{App, Mode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};
use throbber_widgets_tui::Throbber;

pub fn ui(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // Help popup
    if app.show_help {
        let block = Block::default()
            .title(" Help ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);
        let area = centered_rect(60, 65, size);
        f.render_widget(Clear, area);
        let help_text = "Controls: \n\nGeneral:\n Ctrl+o: Model Select\n Ctrl+r: Session Manager\n Ctrl+s: System Prompt\n Ctrl+l: Clear History\n F1: Help\n\nInsert Mode:\n Enter: Send Message\n Shift+Enter: New Line\n Esc: Switch to Normal Mode\n\nNormal Mode:\n j/k: Scroll\n i: Switch to Insert Mode\n q: Quit\n\nSession Manager:\n Enter: Select\n c: Create New\n d: Delete\n\nModel Select:\n Enter: Select\n p: Pull New Model\n d: Delete Model";
        f.render_widget(Paragraph::new(help_text).block(block), area);
        return;
    }

    match app.mode {
        Mode::Insert | Mode::Normal => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header
                    Constraint::Min(1),    // History
                    Constraint::Length(
                        (3 + app.input.lines().len().saturating_sub(1) as u16).min(10),
                    ), // Input grows to max 10 lines
                ])
                .split(size);

            // Header
            let model_name = app
                .models
                .get(app.selected_model)
                .map(|s| s.as_str())
                .unwrap_or("No Model");

            let title = if let Some((msg, time)) = &app.notification {
                if time.elapsed().as_secs() < 3 {
                     format!(" {} ", msg)
                } else {
                     app.notification = None; // Clean up old notification during render (safe enough)
                     format!(
                        " Ollama TUI - {} - Session: {} - Tokens: {}/{} (F1 for Help) ",
                        model_name, app.current_session, app.current_token_usage, app.context_token_limit
                    )
                }
            } else if let Some((status, completed, total)) = &app.pull_progress {
                let percent = if let (Some(c), Some(t)) = (completed, total) {
                    if *t > 0 {
                        (*c as f64 / *t as f64 * 100.0) as u16
                    } else {
                        0
                    }
                } else {
                    0
                };
                format!(
                    " Pulling Model: {} ({}%) - {} (F1 for Help) ",
                    status, percent, app.current_session
                )
            } else {
                format!(
                    " Ollama TUI - {} - Session: {} - Tokens: {}/{} (F1 for Help) ",
                    model_name, app.current_session, app.current_token_usage, app.context_token_limit
                )
            };
            
            let header_style = if app.notification.is_some() {
                 Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                 Style::default().fg(Color::Cyan)
            };

            let header_block = Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(header_style)
                .border_type(BorderType::Rounded);

            if let Some((_, Some(completed), Some(total))) = &app.pull_progress {
                // Render Gauge inside header block? Or just title.
                // Let's overlay a gauge if pulling
                if *total > 0 {
                    let gauge = Gauge::default()
                        .block(header_block.clone())
                        .gauge_style(Style::default().fg(Color::Green))
                        .ratio(*completed as f64 / *total as f64);
                    f.render_widget(gauge, chunks[0]);
                } else {
                    f.render_widget(header_block, chunks[0]);
                }
            } else {
                f.render_widget(header_block, chunks[0]);
            }

            // History (Bubbles)
            let history_area = chunks[1];
            let width = history_area.width;
            let bubble_max_width = (width as f32 * 0.90) as u16;

            if app.messages.is_empty() {
                let empty_text = format!("Start a conversation by typing a message below.\n(Ctrl+o: Model, Ctrl+r: Sessions, Ctrl+s: System Prompt)");
                let p = Paragraph::new(empty_text)
                    .alignment(ratatui::layout::Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray));

                // Centered vertically by using a tall rect and internal newlines
                let area = centered_rect(80, 50, history_area);
                f.render_widget(p, area);
            }

            // 1. Calculate layouts
            // We need to know the height of every message to handle scrolling correctly.
            // (Height, Option<Text>, IsThinking)
            let mut calculated_msgs: Vec<(u16, Option<Text>, bool)> = Vec::new();
            let mut total_height: u16 = 0;

            let msg_count = app.messages.len();
            for (i, msg) in app.messages.iter().enumerate() {
                // Thinking condition: last message is assistant and content is empty (or just whitespace) AND app is loading
                let is_thinking = app.loading
                    && i == msg_count - 1
                    && msg.role == "assistant"
                    && msg.content.trim().is_empty();

                if is_thinking {
                    let height = 3;
                    calculated_msgs.push((height, None, true));
                    total_height += height;
                } else {
                    let content_to_render = if msg.tool_name.is_some() {
                         insert_soft_hyphens(&msg.content)
                    } else {
                        msg.content.clone()
                    };

                    let md_borrowed = tui_markdown::from_str(&content_to_render);
                    let md = to_owned_text(md_borrowed);
                    
                    let content_width = bubble_max_width.saturating_sub(2);
                    let height = estimate_wrapped_height(&md, content_width) + 2;

                    calculated_msgs.push((height, Some(md), false));
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
                 for (i, (height, _, _)) in calculated_msgs.iter().enumerate() {
                      if i == selected_idx {
                          let msg_top = current_y_offset;
                          let msg_bottom = current_y_offset + height;
                          let viewport_h = history_area.height;
                          
                          if msg_top < app.vertical_scroll {
                               app.vertical_scroll = msg_top;
                          } else if msg_bottom > app.vertical_scroll + viewport_h {
                               app.vertical_scroll = msg_bottom.saturating_sub(viewport_h);
                          }
                          break;
                      }
                      current_y_offset += height + 1; // +1 for margin
                 }
            }

            // Render Visible Bubbles
            let mut current_y = -(app.vertical_scroll as i32);

            for (i, (height, text_opt, is_thinking)) in calculated_msgs.into_iter().enumerate() {
                let msg_role = &app.messages[i].role;
                let is_user = msg_role == "user";
                let bubble_height = height;

                if current_y + (bubble_height as i32) > 0 && current_y < (viewport_height as i32) {
                    let bubble_width = bubble_max_width;
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

                        let rect =
                            Rect::new(history_area.x + x, visible_y, bubble_width, visible_height);

                        let (border_color, title) = if is_user {
                            (Color::Green, " You ".to_string())
                        } else if app.messages[i].tool_name.is_some() {
                             (Color::Magenta, format!(" Tool Output: {} ", app.messages[i].tool_name.as_deref().unwrap_or("Unknown")))
                        } else {
                            (Color::Cyan, " AI ".to_string())
                        };

                        let (border_style, final_title) = if Some(i) == app.selected_message_index {
                             (Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD), format!("{} [SELECTED] ", title))
                        } else {
                             (Style::default().fg(border_color), title)
                        };

                        let block = Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(border_style)
                            .title(final_title);

                        if is_thinking {
                            let throbber = Throbber::default().label("Thinking...").throbber_style(
                                Style::default()
                                    .fg(Color::LightCyan)
                                    .add_modifier(Modifier::BOLD),
                            );

                            f.render_widget(block, rect);
                            let inner_area = Rect {
                                x: rect.x + 1,
                                y: rect.y + 1,
                                width: rect.width.saturating_sub(2),
                                height: rect.height.saturating_sub(2),
                            };
                            f.render_stateful_widget(throbber, inner_area, &mut app.spinner_state);
                        } else if let Some(text) = text_opt {
                             // If it's a tool output, we might want to soft-wrap
                            let display_text = if app.messages[i].tool_name.is_some() {
                                // Extract raw text if possible, or just use what we have. 
                                // Actually, `text_opt` is `Text`, we might need to process the raw content again 
                                // or just hack it here. 
                                // Better: modifying `calculated_msgs` loop might be cleaner, but let's see.
                                // The `text` here is already parsed markdown. 
                                // For tool output (which is usually code block or raw text), tui-markdown might have wrapped it in code blocks.
                                
                                // Actually, let's look at where `text` usually comes from.
                                // It comes from `tui_markdown::from_str(&msg.content)`.
                                // If we want soft wrapping, we should probably do it BEFORE markdown parsing 
                                // OR manually construct the Paragraph for tools without markdown parsing if we want raw control?
                                // BUT we want markdown for "Tool Call" blockquotes.
                                
                                // Wait, `app.messages[i]` content for Tool Output is just the raw output string usually.
                                // We are rendering it as markdown.
                                // If we insert soft hyphens into the raw string, they might mess up markdown parsing if not careful?
                                // Zero-width space `\u{200B}` is usually fine.
                                
                                // Let's try applying it to the content BEFORE markdown parsing in the calculation loop?
                                // That seems safer and more consistent.
                                text.clone() // Placeholder, we'll change the CALCULATION loop instead.
                            } else {
                                text.clone() 
                            };

                            let scroll_offset = if item_top < area_top as i32 {
                                (area_top as i32 - item_top) as u16
                            } else {
                                0
                            };
                            let p = Paragraph::new(display_text)
                                .block(block)
                                .wrap(Wrap { trim: false })
                                .scroll((scroll_offset, 0));
                            f.render_widget(p, rect);
                        }
                    }
                }
                current_y += bubble_height as i32 + 1;
            }
            
            // "More Content" Indicator
            // Check if we are not at the bottom
            let max_scroll = total_height.saturating_sub(viewport_height);
            if app.vertical_scroll < max_scroll {
                 let indicator_area = Rect::new(history_area.x, history_area.bottom() - 1, width, 1);
                 f.render_widget(Clear, indicator_area); // Clear any bubbles behind it
                 let indicator = Paragraph::new("v More v")
                    .alignment(ratatui::layout::Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray));
                 f.render_widget(indicator, indicator_area);
            }

            // Render Input (TextArea)
            let (input_border_color, input_title) = if let Some(err) = &app.error {
                (Color::Red, format!(" Error: {} ", err))
            } else {
                match app.mode {
                    Mode::Insert => (
                        Color::Green,
                        " Input (Insert Mode) - Esc for Normal ".to_string(),
                    ),
                    Mode::Normal => (Color::Blue, " Input (Normal Mode) - i to Type ".to_string()),
                    Mode::SystemPromptEdit => (
                        Color::Yellow,
                        " Edit System Prompt (Esc to Cancel, Enter to Save) ".to_string(),
                    ),
                    Mode::ModelSelect => (Color::Magenta, " Select Model ".to_string()),
                    Mode::SessionSelect => (Color::Magenta, " Session Manager ".to_string()),
                    Mode::SessionCreate => (Color::Magenta, " Create Session ".to_string()),
                    Mode::ModelPullInput => (Color::Magenta, " Enter Model Name ".to_string()),
                    Mode::ToolConfirmation => (Color::Red, " Confirm Tool Execution ".to_string()),
                }
            };

            match app.mode {
                Mode::Insert => app.input.set_style(Style::default()),
                _ => app
                    .input
                    .set_style(Style::default().add_modifier(Modifier::DIM)),
            }

            app.input.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(input_title)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(input_border_color)),
            );
            f.render_widget(&app.input, chunks[2]);
        }
        Mode::ModelSelect => {
            let area = centered_rect(60, 40, size);
            f.render_widget(Clear, area);

            let block = Block::default()
                .title(" Select Model (p: Pull, d: Delete) ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded);
            let items: Vec<ListItem> = app
                .models
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let s = if i == app.selected_model {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Span::styled(m, s))
                })
                .collect();
            let list = List::new(items).block(block);
            f.render_widget(list, area);
        }
        Mode::SystemPromptEdit => {
            // We reuse the Insert/Normal layout but focus on the system prompt editor
            // Actually, the user says "displays for ... are not centered".
            // Let's make the System Prompt editor a nice centered popup too.
            let area = centered_rect(80, 30, size);
            f.render_widget(Clear, area);

            app.system_prompt_input.set_block(
                Block::default()
                    .title(" Edit System Prompt ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Yellow)),
            );
            f.render_widget(&app.system_prompt_input, area);
        }
        Mode::SessionSelect => {
            let area = centered_rect(60, 50, size);
            f.render_widget(Clear, area);

            let block = Block::default()
                .title(" Session Manager (c: Create, d: Delete, Enter: Select) ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded);

            let items: Vec<ListItem> = app
                .available_sessions
                .iter()
                .map(|s| {
                    let style = if s == &app.current_session {
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD)
                            .add_modifier(Modifier::ITALIC)
                    } else {
                        Style::default()
                    };
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
        Mode::SessionCreate => {
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
        Mode::ModelPullInput => {
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
        Mode::ToolConfirmation => {
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
    }
}

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

/// Inserts zero-width spaces after common separators to allow for soft wrapping
fn insert_soft_hyphens(text: &str) -> String {
    // \u{200B} is ZERO WIDTH SPACE
    text.replace('/', "/\u{200B}")
        .replace('_', "_\u{200B}")
        .replace(',', ",\u{200B}")
        // We can add more if needed, essentially "allow break after these chars"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_soft_hyphens() {
        let input = "path/to/very/long/file_name.txt";
        let output = insert_soft_hyphens(input);
        assert_eq!(output, "path/\u{200B}to/\u{200B}very/\u{200B}long/\u{200B}file_\u{200B}name.txt");
    }
}
