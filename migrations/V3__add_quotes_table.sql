CREATE TABLE quotes (
    channel TEXT NOT NULL,
    username TEXT NOT NULL,
    quote TEXT NOT NULL,
    key TEXT,
    time TEXT,
    PRIMARY KEY(channel, key),
    CONSTRAINT quotes UNIQUE(channel, quote)
);
