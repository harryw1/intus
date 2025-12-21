use std::mem;

#[derive(Debug, PartialEq, Clone)]
pub enum StreamEvent {
    Content(String),
    Thought(String),
}

#[derive(Debug)]
enum State {
    Normal,
    /// We have matched a prefix of "<thought>" but not the whole thing yet.
    /// The string buffer holds the matched prefix.
    MatchingStartTag(String),
    InThought,
    /// We have matched a prefix of "</thought>" but not the whole thing yet.
    /// The string buffer holds the matched prefix.
    MatchingEndTag(String),
}

pub struct MonologueParser {
    state: State,
    start_tag: &'static str,
    end_tag: &'static str,
}

impl MonologueParser {
    pub fn new() -> Self {
        Self {
            state: State::Normal,
            start_tag: "<thought>",
            end_tag: "</thought>",
        }
    }

    pub fn process(&mut self, chunk: &str) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        let mut chars = chunk.chars().peekable();

        while let Some(c) = chars.next() {
            match &mut self.state {
                State::Normal => {
                    if c == '<' {
                        self.state = State::MatchingStartTag("<".to_string());
                    } else {
                        events.push(StreamEvent::Content(c.to_string()));
                    }
                }
                State::MatchingStartTag(buffer) => {
                    buffer.push(c);
                    if buffer == self.start_tag {
                        self.state = State::InThought;
                    } else if self.start_tag.starts_with(buffer.as_str()) {
                        // Continue matching
                    } else {
                        // Mismatch, dump buffer as content and reset
                        events.push(StreamEvent::Content(buffer.clone()));
                        self.state = State::Normal;
                    }
                }
                State::InThought => {
                    if c == '<' {
                        self.state = State::MatchingEndTag("<".to_string());
                    } else {
                        events.push(StreamEvent::Thought(c.to_string()));
                    }
                }
                State::MatchingEndTag(buffer) => {
                    buffer.push(c);
                    if buffer == self.end_tag {
                        self.state = State::Normal;
                    } else if self.end_tag.starts_with(buffer.as_str()) {
                        // Continue matching
                    } else {
                        // Mismatch, dump buffer as thought and reset to InThought
                        events.push(StreamEvent::Thought(buffer.clone()));
                        self.state = State::InThought;
                    }
                }
            }
        }
        events
    }

    /// Flush any remaining buffered characters.
    /// For example, if stream ends with "<tho", we treat it as content.
    pub fn flush(&mut self) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        match &mut self.state {
            State::MatchingStartTag(buffer) => {
                events.push(StreamEvent::Content(mem::take(buffer)));
            }
            State::MatchingEndTag(buffer) => {
                events.push(StreamEvent::Thought(mem::take(buffer)));
            }
            _ => {}
        }
        self.state = State::Normal;
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to join adjacent checks for easier assertion
    fn normalize(events: Vec<StreamEvent>) -> Vec<StreamEvent> {
        let mut normalized = Vec::new();
        for event in events {
            match event {
                StreamEvent::Content(s) => {
                    if let Some(StreamEvent::Content(last)) = normalized.last_mut() {
                        last.push_str(&s);
                    } else {
                        normalized.push(StreamEvent::Content(s));
                    }
                }
                StreamEvent::Thought(s) => {
                    if let Some(StreamEvent::Thought(last)) = normalized.last_mut() {
                        last.push_str(&s);
                    } else {
                        normalized.push(StreamEvent::Thought(s));
                    }
                }
            }
        }
        normalized
    }

    #[test]
    fn test_simple_thought() {
        let mut parser = MonologueParser::new();
        let input = "Hello <thought>I am thinking</thought> world";
        let events = parser.process(input);
        let events = normalize(events);
        
        assert_eq!(events, vec![
            StreamEvent::Content("Hello ".to_string()),
            StreamEvent::Thought("I am thinking".to_string()),
            StreamEvent::Content(" world".to_string()),
        ]);
    }

    #[test]
    fn test_split_start_tag() {
        let mut parser = MonologueParser::new();
        let events1 = parser.process("Hello <thou");
        let events2 = parser.process("ght>thinking</thought>");
        
        let mut all = normalize(events1);
        all.extend(normalize(events2));
        all = normalize(all);

        assert_eq!(all, vec![
            StreamEvent::Content("Hello ".to_string()),
            StreamEvent::Thought("thinking".to_string()),
        ]);
    }

    #[test]
    fn test_split_end_tag() {
        let mut parser = MonologueParser::new();
        let input = "<thought>thinking</thou";
        let events1 = parser.process(input);
        let events2 = parser.process("ght>Done");
        
        let mut all = normalize(events1);
        all.extend(normalize(events2));
        all = normalize(all);

        assert_eq!(all, vec![
            StreamEvent::Thought("thinking".to_string()),
            StreamEvent::Content("Done".to_string()),
        ]);
    }

    #[test]
    fn test_incomplete_tag_flush() {
        let mut parser = MonologueParser::new();
        let events = parser.process("Hello <thou");
        let flush = parser.flush();
        
        let mut all = normalize(events);
        all.extend(normalize(flush));
        all = normalize(all);

        assert_eq!(all, vec![
            StreamEvent::Content("Hello <thou".to_string()),
        ]);
    }
    
    #[test]
    fn test_mismatched_start_tag() {
        let mut parser = MonologueParser::new();
        let input = "Hello <there>"; // <th matches prefix but e fails
        let events = parser.process(input);
        let events = normalize(events);
        
        assert_eq!(events, vec![
            StreamEvent::Content("Hello <there>".to_string()),
        ]);
    }

    #[test]
    fn test_nested_brackets_in_thought() {
        // We do NOT support nested actual tags, but random < > should be handled as text if they don't match tag
        let mut parser = MonologueParser::new();
        let input = "<thought>Look at this: <Vector></thought>";
        let events = parser.process(input);
        let events = normalize(events);
        
        assert_eq!(events, vec![
            StreamEvent::Thought("Look at this: <Vector>".to_string()),
        ]);
    }
}
