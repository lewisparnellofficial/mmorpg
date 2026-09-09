# Vibe-coded MMORPG

An MMORPG inspired by the MMOs of the late-2000s. This game features tab-target combat, a level editor, addons, and the ability to play with yourself.

The purpose of this project is to explore the capabilities of coding agents on a long-term, underspecified task. Little to no manual verification of code is performed, this is truly peak vibe-coding. As far as I, the author, can tell, the game engine is bevy, hopefully a great choice. I believe that persistence is provided by Postgres. I have also been told that addons are sandboxed and implemented with Luau. But they could also be Wasmi.

## Probable Features

The editor should include tablet support. I remember watching a GDC talk, or some other such video, of Blizzard employees, and they ended up demoing their terrain painting capability. I really want that! I'm aiming for a Warcraft 3 style level editor here: basic scripting, lots of drag and drop, easy enough to author something passable, but expressive enough to author quality too.

## Expectations

You should not expect this to be a quality project, I'm not. Anyone expecting this to be good is a fool. I am currently waiting for this project to implode under its own code-weight.

The current state of this project is very early in development. There has been no attempts to make the project portable to other development environments. If, for some reason, you decide to clone this repo, you'll need to figure it out on your own, or ask 

 

My goal is to develop a stronger intuition over agent efficacy. I mostly want to see what the signs of an overly vibe-coded project are. Right now this game launches, and when the agent breaks it, they can fix it in a few minutes. I'm planning on taking this project to the point where the agent has to try for hours to restore the game to a playable state after moving an image in this code's metaphorical Word document.

So again, do not expect that this project renders out a playable game upon its v1. I would not read this project's code and try to intuit a sense of how you should implement your own MMORPG. You could instead review one of the many different private servers for WoW, FFXI, I believe that there's one for EVE now, and many more that I cannot remember. Regardless, nothing in this project is meaningfully human authored.

## Technical Stuff

The project uses Rust, I'm certain of this. There may be some project specific testing tools to let agents test the game (these little guys just love opening and closing the window to make sure it works) in the future.