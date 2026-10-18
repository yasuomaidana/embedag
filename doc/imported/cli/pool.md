<!-- Imported from Dendrite docs/cli/pool.md @ 69ea048 on 2026-10-18. Source of truth for this repo's scope; keep in sync with the Dendrite copy until the extraction umbrella lands. -->

---
id: pool
title: "dendrite pool"
sidebar_label: "pool"
---

# dendrite pool

Multi-directory dependency inference & topological DAG solver

## Category

Soma & Axon Hillock

## Aliases

- `connectome-pool`
- `plexus-pool`
- `connectome`
- `plexus`
- `plx`

## Subcommands

| Subcommand | Description |
| :--- | :--- |
| `add` | Add repository directory to connectome pool |
| `rm` | Remove directory from pool |
| `list` | List registered directories and dependencies |
| `resolve` | Compute topological DAG execution order across directories |

## Flags

None

## Examples

```bash
dendrite pool list
dendrite pool resolve
```
