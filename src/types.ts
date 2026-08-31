export type SplitDirection = "horizontal" | "vertical";
export type ReorderDirection = "earlier" | "later";

export type LayoutNode =
  | { kind: "panel"; panelId: string }
  | {
      kind: "split";
      id: string;
      direction: SplitDirection;
      ratio: number;
      first: LayoutNode;
      second: LayoutNode;
    };

export interface TerminalPanelModel {
  id: string;
  title: string;
  label?: string | null;
  profileId: string;
}

export interface Workspace {
  id: string;
  name: string;
  panels: TerminalPanelModel[];
  layout: LayoutNode | null;
}

export interface Area {
  id: string;
  name: string;
  rootPath: string;
  workspaces: Workspace[];
}

export interface ProfileView {
  id: string;
  name: string;
  builtIn: boolean;
  available: boolean;
  configured: boolean;
}

export interface AppSnapshot {
  areas: Area[];
  profiles: ProfileView[];
  activeTerminalIds: string[];
}

export interface TerminalOutput {
  terminalId: string;
  data: string;
}

export interface TerminalExit {
  terminalId: string;
}
