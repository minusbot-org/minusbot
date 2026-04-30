---
id: example
name: Example Skill
description: An example skill to demonstrate the minusbot skills system.
tags: [example, demo]
---

# Example Skill

This is an example skill file for minusbot.

## What are skills?

Skills are Markdown knowledge files that can be loaded into a chat to give the AI assistant additional context and specialized knowledge.

## How to use skills

1. List available skills: `/skills list`
2. Load a skill: `/skills load example`
3. Unload a skill: `/skills unload example`
4. Search skills: `/skills search demo`

## Creating your own skills

Create a new `.md` file in the skills directory with YAML frontmatter:

```yaml
---
id: my-skill
name: My Custom Skill
description: Description of what this skill covers.
tags: [tag1, tag2]
---
```

Then add your knowledge content below the frontmatter.
