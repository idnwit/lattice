# 07 — Media input

Status: draft
Owns: images and voice sent to agents.

## Summary

Images go to the worker over P2P, then into the agent in the form it accepts.
Voice is dictation into the prompt.

## Images

1. Sources: `⌘V` paste, drag-drop, mobile attach sheet (Paste image from clipboard, Photo library, Take photo, File from <worker>…).
2. Upload shows progress: `p2p → build-01 · 64%`.
3. TTY view: the CLI receives an `[Image #n]` paste. A chip shows `[Image #n] name · size`.
4. Visual view: the image becomes a composer attachment with a remove `×`.
5. Image files on the worker can be referenced the same way. TBD: max image size and accepted formats.

## Voice

1. The mic shows only when the agent supports voice input.
2. Desktop: `⌥Space` toggles listening. State text `listening · ⌥Space`.
3. Mobile: hold to talk, release to stop, slide up to lock.
4. Transcript lands in the composer for review. It is not sent automatically.

## Open questions

- TBD: speech-to-text runs where (deck OS, worker, cloud)?
- TBD: how is "agent supports voice input" declared?
