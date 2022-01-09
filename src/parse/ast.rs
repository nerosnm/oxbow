#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Command {
    Quote(Quote),
    Meta(MetaCommand),
    Search(Search),
    PotentialUser(PotentialUser),
    Help(Help),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Quote {
    Add {
        username: String,
        key: Option<String>,
        text: String,
    },
    Get {
        key: String,
    },
    Random,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetaCommand {
    pub trigger: String,
    pub response: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Search {
    Search,
    Lower {
        word: String,
        distance: Option<usize>,
    },
    Upper {
        word: String,
        distance: Option<usize>,
    },
    Found,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PotentialUser {
    pub trigger: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Help {
    Quote,
}
