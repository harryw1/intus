use crate::app::{App, Mode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Row, Table, Wrap},
    Frame,
};
use throbber_widgets_tui::Throbber;
use comrak::{parse_document, Arena, Options, nodes::{AstNode, NodeValue, ListType}, markdown_to_html};

fn markdown_to_text(markdown: &str, width: u16) -> Text<'static> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    
    let root = parse_document(&arena, markdown, &options);
    let mut lines = Vec::new();
    let mut current_line = Vec::new();

    fn walk<'a>(
        node: &'a AstNode<'a>,
        current_line: &mut Vec<Span<'static>>,
        lines: &mut Vec<ratatui::text::Line<'static>>,
        style: Style,
        width: u16,
        indent: usize,
        options: &Options,
    ) {
        match &node.data.borrow().value {
            NodeValue::Text(text) => {
                current_line.push(Span::styled(text.clone(), style));
            }
            NodeValue::Code(code) => {
                current_line.push(Span::styled(format!(" {} ", code.literal), style.bg(Color::Rgb(65, 69, 89)).fg(Color::Rgb(242, 213, 207))));
            }
            NodeValue::Emph => {
                for child in node.children() {
                    walk(child, current_line, lines, style.add_modifier(Modifier::ITALIC), width, indent, options);
                }
            }
            NodeValue::Strong => {
                for child in node.children() {
                    walk(child, current_line, lines, style.add_modifier(Modifier::BOLD), width, indent, options);
                }
            }
            NodeValue::Link(link) => {
                 current_line.push(Span::styled("[", style));
                 for child in node.children() {
                     walk(child, current_line, lines, style.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED), width, indent, options);
                 }
                 current_line.push(Span::styled(format!("]({})", link.url), style.fg(Color::DarkGray)));
            }
            NodeValue::Paragraph => {
                for child in node.children() {
                    walk(child, current_line, lines, style, width, indent, options);
                }
                if !current_line.is_empty() {
                    lines.push(ratatui::text::Line::from(current_line.drain(..).collect::<Vec<_>>()));
                }
                lines.push(ratatui::text::Line::from(""));
            }
            NodeValue::Heading(h) => {
                let h_style = style.add_modifier(Modifier::BOLD).fg(match h.level {
                    1 => Color::Magenta,
                    2 => Color::Blue,
                    _ => Color::Cyan,
                });
                current_line.push(Span::styled("#".repeat(h.level as usize) + " ", h_style));
                for child in node.children() {
                    walk(child, current_line, lines, h_style, width, indent, options);
                }
                lines.push(ratatui::text::Line::from(current_line.drain(..).collect::<Vec<_>>()));
                lines.push(ratatui::text::Line::from(""));
            }
            NodeValue::List(l) => {
                let mut item_index = l.start;
                for child in node.children() {
                    let prefix = match l.list_type {
                        ListType::Bullet => " • ".to_string(),
                        ListType::Ordered => format!(" {}. ", item_index),
                    };
                    let mut item_line = Vec::new();
                    item_line.push(Span::styled(prefix, style.fg(Color::Yellow)));
                    
                    // Walk list item content
                    walk(child, &mut item_line, lines, style, width, indent + 3, options);
                    
                    if !item_line.is_empty() {
                         lines.push(ratatui::text::Line::from(item_line));
                    }
                    item_index += 1;
                }
                lines.push(ratatui::text::Line::from(""));
            }
            NodeValue::Item(_) => {
                for child in node.children() {
                    walk(child, current_line, lines, style, width, indent, options);
                }
            }
            NodeValue::CodeBlock(cb) => {
                lines.push(ratatui::text::Line::from(vec![Span::styled(format!("── {} ──", cb.info), style.fg(Color::DarkGray))]));
                for line in cb.literal.lines() {
                    lines.push(ratatui::text::Line::from(vec![Span::styled(line.to_string(), style.fg(Color::Rgb(166, 209, 137)))]));
                }
                lines.push(ratatui::text::Line::from(vec![Span::styled("────────────────", style.fg(Color::DarkGray))]));
                lines.push(ratatui::text::Line::from(""));
            }
            NodeValue::Table(_) => {
                let mut buf = Vec::new();
                comrak::format_commonmark(node, options, &mut buf).unwrap();
                let commonmark = String::from_utf8(buf).unwrap();
                
                let html = markdown_to_html(&commonmark, options);
                let text = html2text::from_read(html.as_bytes(), width.saturating_sub(4) as usize);
                
                for line in text.lines() {
                    lines.push(ratatui::text::Line::from(vec![Span::styled(line.to_string(), style.fg(Color::Rgb(129, 200, 190)))]));
                }
                lines.push(ratatui::text::Line::from(""));
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => {
                // Ignore or handle? Usually handled by Line::from
            }
            _ => {
                for child in node.children() {
                    walk(child, current_line, lines, style, width, indent, options);
                }
            }
        }
    }

    for child in root.children() {
        walk(child, &mut current_line, &mut lines, Style::default(), width, 0, &options);
    }
    
    // Clean up trailing empty lines
    while let Some(last) = lines.last() {
        if last.width() == 0 {
            lines.pop();
        } else {
            break;
        }
    }

    Text::from(lines)
}

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

    let title_text = format!(" Intus | Model: {} ", model_name);
    
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
    } else {
        // Priority: Critical Health Check Failure > Pull Progress > Normal Status
        let critical_health = app.health_status.iter().find(|s| matches!(s.status, crate::health::HealthStatus::Critical(_)));
        
        if let Some(fail) = critical_health {
            if let crate::health::HealthStatus::Critical(msg) = &fail.status {
                format!(" CRITICAL ERROR: {} - {} ", fail.name, msg)
            } else {
                String::new() // Should not reach here
            }
        } else if let Some((status, completed, total)) = &app.pull_progress {
             let percent = if let (Some(c), Some(t)) = (completed, total) {
                 if *t > 0 { (*c as f64 / *t as f64 * 100.0) as u16 } else { 0 }
             } else { 0 };
             format!(" Pulling: {} ({}%) ", status, percent)
        } else {
            let limit = app.model_context_limit.unwrap_or(app.context_token_limit);
            
            // Check warnings
            let warning_health = app.health_status.iter().find(|s| matches!(s.status, crate::health::HealthStatus::Warning(_)));
            let warning_text = if let Some(warn) = warning_health {
                 if let crate::health::HealthStatus::Warning(msg) = &warn.status {
                     format!(" | Warn: {} - {} ", warn.name, msg)
                 } else { String::new() }
            } else { String::new() };

            format!(
                " {} | Session: {} | Tokens: {}/{}{} | F1: Help ",
                mode_str, app.current_session, app.current_token_usage, limit, warning_text
            )
        }
    };
    
    // Style override for Critical
    let style = if app.health_status.iter().any(|s| matches!(s.status, crate::health::HealthStatus::Critical(_))) {
         Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
         app.theme.status_bar()
    };
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
    // Tuple: (height, Option<Text>, Option<String>, u16)
    // Option<String> is the Throbber Label (Some("Thinking...") or Some("Executing Tool..."))
    let mut calculated_msgs: Vec<(u16, Option<Text>, Option<String>, u16)> = Vec::new();
    let mut total_height: u16 = 0;

    let msg_count = app.messages.len();
    for (i, msg) in app.messages.iter().enumerate() {
        let is_thinking = app.loading
            && i == msg_count - 1
            && msg.role == "assistant"
            && msg.content.trim().is_empty()
            && msg.thought.is_none(); // Only show generic thinking if no specific thought exists yet

        if is_thinking {
            let height = 3;
            // Thinking bubble fixed width or dynamic? Let's make it fixed wide enough for "Thinking..."
            let bubble_width = 16.min(max_available_width); 
            calculated_msgs.push((height, None, Some("Thinking...".to_string()), bubble_width));
            total_height += height;
        } else {
            // Render Thought Bubble if present
            if let Some(thought) = &msg.thought {
                if !thought.trim().is_empty() {
                    let thought_text = markdown_to_text(thought, max_available_width);
                     
                    // Calculate thought width logic (same as content)
                    let thought_text_width = thought_text.lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16;
                    let required_width = thought_text_width.saturating_add(4);
                    let bubble_width = required_width.clamp(14, max_available_width);
                    let wrapping_width = bubble_width.saturating_sub(4);
 
                    let height = estimate_wrapped_height(&thought_text, wrapping_width) + 2;
                    
                    // Use a special marker label for "Thought" rendering downstream
                    calculated_msgs.push((height, Some(thought_text), Some("Internal Thought".to_string()), bubble_width));
                    total_height += height;
                    
                    // Add small margin?
                    // total_height += 1;
                }
            }

            let content_to_render = if msg.tool_name.is_some() {
                 let raw_content = insert_soft_hyphens(&msg.content);
                 truncate_content(&raw_content, 30)
            } else if let Some(calls) = &msg.tool_calls {
                 if msg.content.trim().is_empty() {
                     let names: Vec<String> = calls.iter().map(|c| c.function.name.clone()).collect();
                     format!("**Using Tool:** `{}`", names.join("`, `"))
                 } else {
                     msg.content.clone()
                 }
            } else {
                msg.content.clone()
            };

            // Only render content if it's not empty OR if there are no thoughts (to avoid invisible messages)
            // But sometimes assistant message starts with thought and has no content yet.
            if !content_to_render.is_empty() || (msg.role == "assistant" && msg.thought.is_none()) {
                let text = markdown_to_text(&content_to_render, max_available_width);
                
                // Calculate content width
                let content_text_width = text.lines.iter().map(|l| l.width()).max().unwrap_or(0) as u16;
                
                let required_width = content_text_width.saturating_add(4); // +4 for safe padding inside borders
                let bubble_width = required_width.clamp(14, max_available_width);
                let wrapping_width = bubble_width.saturating_sub(4); // Inner width for text wrapping

                let height = estimate_wrapped_height(&text, wrapping_width) + 2; // +2 for border height

                calculated_msgs.push((height, Some(text), None, bubble_width));
                total_height += height;
            }
        }
    }

    // Add margins (1 line between bubbles)
    if app.is_tool_executing {
        let height = 3;
        let bubble_width = 22.min(max_available_width);
        calculated_msgs.push((height, None, Some("Executing Tool...".to_string()), bubble_width));
        total_height += height;
    }

    // Add margins (1 line between bubbles)
    if !calculated_msgs.is_empty() {
        total_height += (calculated_msgs.len() as u16).saturating_sub(1);
    }

    // Scroll Logic
    let viewport_height = history_area.height;
    let max_scroll = total_height.saturating_sub(viewport_height);

    if app.auto_scroll {
        if total_height > viewport_height {
            app.vertical_scroll = max_scroll;
        } else {
            app.vertical_scroll = 0;
        }
    } else {
        if app.vertical_scroll >= max_scroll {
            app.vertical_scroll = max_scroll;
            app.auto_scroll = true;
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

    for (i, (height, text_opt, throbber_label, bubble_width)) in calculated_msgs.into_iter().enumerate() {
        // Safe access to messages, but handle virtual bubbles (throbber without message index)
        let (_msg_role, is_user, is_tool) = if i < app.messages.len() {
             let m = &app.messages[i];
             (&m.role, m.role == "user", m.tool_name.is_some())
        } else {
             // Virtual bubble (Thinking or Tool Execution) logic
             // "Thinking..." replaces the empty assistant message in the loop, so it has index i < messages.len()
             // BUT "Executing Tool..." is APPENDED, so `i` might be >= messages.len().
             // Actually, `Thinking` block in loop REPLACES the content display but consumes the message index.
             // `Executing Tool` block is NEW.
             // So if i >= messages.len(), it's the extra throbber.
             (&"assistant".to_string(), false, false)
        };
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
                      // Be careful with index access here if i >= messages.len()
                      let tool_name = if i < app.messages.len() {
                          app.messages[i].tool_name.as_deref().unwrap_or("Unknown")
                      } else {
                          "Unknown"
                      };
                      (app.theme.tool_bubble_fg, format!(" Tool Output: {} ", tool_name))
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

                if let Some(label) = throbber_label {
                    if label == "Internal Thought" {
                        // Render thought bubble
                        // Use a specific style for thoughts (e.g., italic, dim)
                         let thought_block = Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(Color::DarkGray)) 
                            .title(Span::styled(" Thought ", Style::default().add_modifier(Modifier::ITALIC)));
                        
                        f.render_widget(thought_block.clone(), rect);
                        let inner_area = thought_block.inner(rect);
                        
                        if let Some(text) = text_opt {
                             // Apply dim/italic style to thought text
                             let mut styled_text = text.clone();
                             for line in &mut styled_text.lines {
                                 for span in &mut line.spans {
                                     span.style = span.style.add_modifier(Modifier::ITALIC).fg(Color::DarkGray);
                                 }
                             }
                             
                             let p = Paragraph::new(styled_text)
                                 .wrap(Wrap { trim: false })
                                 .alignment(ratatui::layout::Alignment::Left); // Thoughts always left
                             f.render_widget(p, inner_area);
                        }

                    } else {
                        // Normal Throbber (Thinking... / Executing Tool...)
                        let throbber = Throbber::default().label(label.clone()).throbber_style(
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
                    }
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
        Row::new(vec![" Shift/Alt+Enter", "New Line"]),
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

fn insert_soft_hyphens(text: &str) -> String {
    // \u{200B} is ZERO WIDTH SPACE
    text.replace('/', "/\u{200B}")
        .replace('_', "_\u{200B}")
        .replace(',', ",\u{200B}")
}

fn truncate_content(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > max_lines {
        let truncated = lines[..max_lines].join("\n");
        let _remaining = lines.len() - max_lines;
        format!("{}\n\n... [Output truncated associated with tool usage. Total: {} lines]", truncated, lines.len())
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_content_short() {
        let text = "Line 1\nLine 2\nLine 3";
        // Should not truncate
        assert_eq!(truncate_content(text, 5), text);
    }

    #[test]
    fn test_truncate_content_long() {
        let text = "1\n2\n3\n4\n5\n6";
        // Truncate to 3 lines
        let truncated = truncate_content(text, 3);
        assert!(truncated.starts_with("1\n2\n3"));
        assert!(truncated.contains("... [Output truncated associated with tool usage. Total: 6 lines]"));
    }

    #[test]
    fn test_truncate_content_exact() {
         let text = "1\n2\n3";
         assert_eq!(truncate_content(text, 3), text);
    }
}
