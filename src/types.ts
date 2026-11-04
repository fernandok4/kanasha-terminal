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
  member?: TeamMember | null;
}

export interface Assignment {
  workContexts?: WorkContext[];
  deliveries?: Delivery[];
  stepIds?: string[];
  taskCode?: string | null;
  freshContext?: boolean;
  threadId?: string | null;
  training?: boolean;
  id: string;
  message: string;
  assignedBy: string | null;
  status: "queued" | "running" | "done" | "failed" | "interrupted" | "uncertain";
  result: string;
  createdAt: number;
  updatedAt: number;
  turnId: string | null;
}
export interface AgentExecution {
  level: "junior" | "mid" | "senior" | "specialist" | "manager" | "custom";
  model: string;
  reasoningEffort: string;
}
export interface CodexModel {
  model: string;
  displayName: string;
  defaultReasoningEffort: string;
  supportedReasoningEfforts: { reasoningEffort: string; description: string }[];
}
export interface AgentSoul {
  content: string;
  revision: number;
  updatedAt: number | null;
  updatedBy: string | null;
  approved: { content: string; revision: number; approvedAt: number; approvedBy: string } | null;
}
export interface TeamMember {
  execution?: AgentExecution;
  professionalProfile?: { name: string; personality: string; content: string; specialty: string; generatedAt: number } | null;
  automaticName?: boolean;
  soul?: AgentSoul;
  personality?: string;
  training?: "awaiting" | "learning" | "review" | "ready";
  memory?: { content: string; revision: number; updatedAt: number | null; updatedBy: string | null };
  specialty: string;
  instructions: string;
  manager: boolean;
  threadId: string | null;
  paused: boolean;
  taskQueue?: string[];
  contextTaskCode?: string | null;
  managerTaskCode?: string | null;
  deletedManagerConversations?: string[];
  managerConversations?: Record<string, { threadId: string | null; instructionsHash: string | null }>;
  jobs: Assignment[];
}
export interface MemberRuntime {
  backgroundConversations?: Record<string, { working: boolean; waiting: boolean }>;
  model?: string | null;
  reasoningEffort?: string | null;
  instructionsRevision?: string | null;
  instructionsPending?: boolean;
  instructionsError?: string | null;
  connected: boolean;
  turnId: string | null;
  activity: string;
  updatedAt: number;
  request?: {
    _connection?: string;
    id: number | string;
    method: string;
    params: { questions?: { id: string; header: string; question: string; options?: { label: string; description: string }[] }[]; command?: string; reason?: string };
  } | null;
}

export type TaskStatus =
  | "not_started"
  | "in_progress"
  | "waiting"
  | "blocked"
  | "done";

export interface TaskStep {
  id: string;
  title: string;
  status: TaskStatus;
}

export interface TaskState {
  status: TaskStatus;
  summary?: string;
  steps?: TaskStep[];
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

export type WorkspaceKind = "normal" | "team";

export function isTeamWorkspace(workspace: Workspace): boolean {
  return workspace.kind ? workspace.kind === "team" : workspace.panels.some(panel => Boolean(panel.member));
}

export interface Workspace {
  kind?: WorkspaceKind;
  id: string;
  name: string;
  task?: TaskState;
  tasks?: { taskCode: string; state: TaskState }[];
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
  memberStates?: Record<string, MemberRuntime>;
}

export interface TerminalOutput {
  terminalId: string;
  data: string;
}

export interface TerminalExit {
  terminalId: string;
}

export interface DeliveryGitSnapshot {
  observedAt: number;
  checkoutPath: string;
  repositoryRoot: string | null;
  gitCommonDir: string | null;
  branch: string | null;
  head: string | null;
  status: string[];
  trackedDiffSha256: string | null;
  stagedDiffSha256: string | null;
  limitations: string[];
}
export interface WorkContext {
  id: string;
  registeredBy: string;
  request: { assignmentId: string; requestId: string; checkoutPath: string; note: string };
  baseline: DeliveryGitSnapshot;
}
export interface DeliveryEvidence {
  id: string;
  kind: "code" | "gitlab" | "merge_request" | "test" | "review" | "deployment" | "investigation";
  reference: string;
  version: string;
  details: string;
}
export interface DeliveryReview {
  revision: number;
  reviewedBy: string;
  reviewedAt: number;
  assessment: {
    deliveryRevision: number;
    decision: "accepted" | "changes_requested" | "inconclusive";
    summary: string;
    checks: { evidenceId: string; state: "verified" | "unavailable" | "divergent" | "not_checked"; source: string; reference: string; version: string; note: string }[];
  };
}
export interface Delivery {
  revision: number;
  submittedBy: string;
  submittedAt: number;
  declaration: {
    kind: "code_change" | "investigation" | "validation" | "documentation";
    summary: string;
    limitations: string;
    contextIds: string[];
    evidence: DeliveryEvidence[];
  };
  repositories: { contextId: string; finalSnapshot: DeliveryGitSnapshot; comparison: { commits: string[]; committedFiles: string[]; limitations: string[] } }[];
  reviews: DeliveryReview[];
}
