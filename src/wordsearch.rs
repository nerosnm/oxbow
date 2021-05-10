lazy_static::lazy_static! {
    static ref ALL: Vec<String> = {
        include_str!("../assets/words.txt").split_whitespace().map(ToOwned::to_owned).collect()
    };
}

pub struct WordSearch {
    possible: &'static [String],
}

impl WordSearch {
    pub fn new() -> Self {
        let mut ws = Self { possible: &[] };
        ws.reset();
        ws
    }

    pub fn reset(&mut self) {
        self.possible = &ALL[..];
    }

    pub fn guess(&self) -> &str {
        let mid = self.possible.len() / 2;
        &self.possible[mid]
    }

    pub fn set_lower(&mut self, word: &str) {
        if let Some(index) = self.possible.iter().position(|ea| *ea == word) {
            self.possible = &self.possible[(index + 1)..];
        }
    }

    pub fn set_upper(&mut self, word: &str) {
        if let Some(index) = self.possible.iter().rposition(|ea| *ea == word) {
            self.possible = &self.possible[..index];
        }
    }
}
