CREATE TABLE commands (
    channel TEXT NOT NULL,
    trigger TEXT NOT NULL,
    response TEXT NOT NULL,
    PRIMARY KEY(channel, trigger)
);
