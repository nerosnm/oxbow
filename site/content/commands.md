+++
title = "Commands"
+++

# Commands

## Quotes

The `!quote` command allows you to save and retrieve quotes from users in the current Twitch 
channel.

### Add a Quote

To add a new quote, mention the user who said it and put the text in double quotes:

```
!quote @fisken_ai "the run rames"
```

The quote will be added to the list of quotes for the current Twitch channel, and it may come up 
when Oxbow is asked for a random quote.

If you want to have the option to retrieve a specific quote later, rather than just waiting for it 
to come up at random, you'll have to include a key when you add it:

```
!quote @NinthRoads #unwatchable "this stream is borderline unwatchable"
```

### Get a Random Quote

To get a random quote from the current channel, just run the command with no arguments:

```
!quote
```

### Get a Quote by Key

To get a specific quote (if it has a key), provide its key as an argument:

```
!quote #run
```

## Custom Commands

The `!command` command allows you to add simple custom commands, which respond to a trigger with
some text. For the moment, this functionality is unrestricted, which allows any user in the channel
to create or update custom commands freely.

### Set

To set or update the response for a specific trigger, provide the trigger followed by its response
in quotes:

```
!command polls "kiss me on the mouth in a fraternal soviet kiss, comrade"
```

### Run

To run a custom command, just prefix the trigger with the bot's prefix, `!`, for example:

```
!snip
```
