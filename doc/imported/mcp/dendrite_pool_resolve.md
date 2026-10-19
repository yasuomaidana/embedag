<!-- Imported from Dendrite docs/mcp/dendrite_pool_resolve.md @ 9cbfbeb on 2026-10-18. Source of truth for this repo's scope; keep in sync with the Dendrite copy until the extraction umbrella lands. -->

---
id: dendrite_pool_resolve
title: "dendrite_pool_resolve"
sidebar_label: "dendrite_pool_resolve"
---

# dendrite_pool_resolve

Computes deterministic topological DAG across multi-directory pool projects and detects circular dependency hazards

## Input Schema

```json
{
  "properties": {
    "directories": {
      "description": "Optional ad-hoc list of directory paths to resolve directly without loading pool config",
      "items": {
        "type": "string"
      },
      "type": "array"
    }
  },
  "type": "object"
}
```

## Example Call

### JSON-RPC

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "arguments": {
      "directories": [
        "item1"
      ]
    },
    "name": "dendrite_pool_resolve"
  }
}
```

### CLI

```bash
dendrite mcp call dendrite_pool_resolve '{"directories":["item1"]}'
```
