# Working on code

## Plan first

Before implementing a new feature, module or daemon, or any change that is more than a small fix, present a plan and
wait for my approval. Do not write the code before I approve.

When there is more than one reasonable design, propose several, with their trade-offs and your recommendation, and let
me choose.

## Keep docs and code in sync

`docs/` is the specification. Every change that alters behaviour updates the affected docs in the same change.
When you notice docs and code disagree, even outside your task, tell me.

## Conflicts with the docs

If a task would make the code contradict the docs, stop before implementing it. Tell me:

- what the docs say (file and line),
- what the task requires instead,
- the options you see.

I decide whether to change the docs, change the task, or do something else. Do not resolve the conflict yourself.

## When a task turns out hard

If during a task you find it is much harder than expected (it needs an architectural change, touches far more code than
planned, or depends on unclear Core behaviour), stop and tell me before writing a lot of code.
Explain the problem and propose solutions.
