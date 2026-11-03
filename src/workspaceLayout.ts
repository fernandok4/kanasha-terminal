import type { LayoutNode, SplitDirection, Workspace } from "./types";

// This is a presentation layout: existing saved splits remain available for
// workspaces without a manager, and no terminal session needs to be recreated.
export function workspaceLayout(workspace: Workspace): LayoutNode | null {
  const manager = workspace.panels.find(panel => panel.member?.manager);
  if (!manager) return workspace.layout;

  const managerNode: LayoutNode = { kind: "panel", panelId: manager.id };
  const others = workspace.panels.filter(panel => panel.id !== manager.id);
  if (!others.length) return managerNode;

  const join = (nodes: LayoutNode[], direction: SplitDirection, id: string): LayoutNode => {
    if (nodes.length === 1) return nodes[0];
    const middle = Math.ceil(nodes.length / 2);
    return {
      kind: "split", id, direction, ratio: middle / nodes.length,
      first: join(nodes.slice(0, middle), direction, `${id}-first`),
      second: join(nodes.slice(middle), direction, `${id}-second`),
    };
  };

  // Favor stacked rows in the narrow employee half. Incomplete rows expand
  // across the available width instead of leaving empty cells.
  const rowCount = Math.ceil(Math.sqrt(others.length));
  const rows: LayoutNode[] = [];
  let offset = 0;
  for (let row = 0; row < rowCount; row += 1) {
    const count = Math.ceil((others.length - offset) / (rowCount - row));
    const cells: LayoutNode[] = others.slice(offset, offset + count)
      .map(panel => ({ kind: "panel", panelId: panel.id }));
    rows.push(join(cells, "vertical", `team-${workspace.id}-row-${row}`));
    offset += count;
  }

  return {
    kind: "split", id: `team-${workspace.id}`, direction: "vertical", ratio: 0.5,
    first: managerNode,
    second: join(rows, "horizontal", `team-${workspace.id}-rows`),
  };
}
