<!-- Imported from Dendrite docs/mcp/dendrite_ticket_smart_select.md @ 9cbfbeb on 2026-10-18. Source of truth for this repo's scope; keep in sync with the Dendrite copy until the extraction umbrella lands. -->

---
id: dendrite_ticket_smart_select
title: "dendrite_ticket_smart_select"
sidebar_label: "dendrite_ticket_smart_select"
---

# dendrite_ticket_smart_select

Smart selects the optimal unblocked ticket to be done based on priority (Urgent > High > Medium > Low), dependency relationships (DAG unblocked check), and downstream unblock impact (critical path weight). If ticket_id is specified by user, evaluates its blockers and selects it.

## Input Schema

```json
{
  "properties": {
    "file": {
      "description": "Optional fallback source markdown file (default: DEV_STATUS.md)",
      "type": "string"
    },
    "milestone": {
      "description": "Optional milestone name or filter",
      "type": "string"
    },
    "ticket_id": {
      "description": "Optional specific ticket ID requested by user (e.g. 'DEN-036')",
      "type": "string"
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
      "file": "DEV_STATUS.md",
      "milestone": "sample",
      "ticket_id": "DEN-985"
    },
    "name": "dendrite_ticket_smart_select"
  }
}
```

### CLI

```bash
dendrite mcp call dendrite_ticket_smart_select '{"file":"DEV_STATUS.md","milestone":"sample","ticket_id":"DEN-985"}'
```
