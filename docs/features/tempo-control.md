# Manual BPM in the app — implemented 2026-10-03

Status: implemented.

## What

An editable BPM field in the transport bar.

## Why

A blank project needs a tempo the person can set without a terminal.

## Design

The field submits a finite number from 20 to 400 BPM through the host's session tempo
command. Return or leaving the field commits; Escape cancels. An edit is saved
and undoable, and changes from the agent update the display when it is not being
edited. Invalid input reports the accepted range without changing the song.

## Done when

Typing a fractional tempo updates the song, undo restores it, invalid input is
refused, and the app builds and shows the control on a scratch project.

## Open questions

None. Tempo maps and time signature changes remain separate backlog work.
