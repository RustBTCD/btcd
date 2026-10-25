# Git

Never run git commands that change the repository: its working tree, index, branches, tags, stash or remotes.
This includes `commit`, `add`, `rm`, `mv`, `restore`, `checkout`, `switch`, `reset`, `revert`, `merge`, `rebase`,
`cherry-pick`, `stash`, `tag`, `branch -d`, `fetch`, `pull` and `push`. To delete or move files, use plain `rm` and
`mv`, not `git rm` or `git mv`.

The only exception is when I ask directly for a specific operation. That permission covers that request only, not later
ones.

Read-only commands are allowed and encouraged: `status`, `log`, `diff`, `show`, `blame`, `grep`, listing branches.
