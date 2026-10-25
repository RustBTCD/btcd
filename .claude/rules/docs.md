---
paths:
  - "**/*.md"
---

# Markdown files

Keep lines at most 120 characters, IntelliJ IDEA's default right margin, so files read without horizontal scrolling.

- Wrap prose at the last word boundary before the limit. A single line break inside a paragraph does not change the
  rendered output.
- Indent continuation lines of a list item to the item's text, as in this list, so they stay part of the item.
- Prefer breaking after a sentence or clause over breaking in the middle of a phrase.
- Never break inside a link, inline code, a URL or a heading.
- Exempt: table rows, code blocks (including Mermaid diagrams) and lines that are long only because of an unbreakable
  link or URL. If a table row gets very long, consider whether the content belongs in a list or prose instead.
