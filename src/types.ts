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

export type TaskStatus =
  | "not_started"
  | "in_progress"
  | "waiting"
  | "blocked"
  | "done";

export interface TaskState {
  status: TaskStatus;
  summary?: string;
  current?: string;
  next?: string;
  blocker?: string;
  updatedAt?: number;
  revision?: number;
}

export type AgentStatus = "idle" | "working" | "waiting" | "blocked" | "done";

export interface AgentState {
  status: AgentStatus;
  current?: string;
  updatedAt?: number;
  revision?: number;
}

export interface Workspace {
  id: string;
  name: string;
  task?: TaskState;
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
  agentStates?: Record<string, AgentState>;
}

export interface TerminalOutput {
  terminalId: string;
  data: string;
}

export interface TerminalExit {
  terminalId: string;
}
