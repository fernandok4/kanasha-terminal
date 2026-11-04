import { invoke } from "@tauri-apps/api/core";
import type {
  AppSnapshot,
  AgentExecution,
  CodexModel,
  ReorderDirection,
  SplitDirection,
} from "./types";

type CreateTerminal = {
  workspaceId: string;
  profileId: string;
  title?: string;
  targetPanelId?: string;
  direction?: SplitDirection;
};

export const terminalApi = {
  listCodexModels: (workspaceId: string) => invoke<CodexModel[]>("list_codex_models", { workspaceId }),
  prepareArea: (areaId: string) => invoke<void>("prepare_area", { areaId }),
  getAreaInstructions: (areaId: string) => invoke<{ path: string; content: string; revision: string }>("get_area_instructions", { areaId }),
  setAreaInstructions: (areaId: string, content: string, expectedRevision: string) => invoke<{ path: string; content: string; revision: string }>("set_area_instructions", { areaId, content, expectedRevision }),
  hireAgent: (input: { workspaceId: string; name?: string; specialty: string; instructions?: string; execution?: AgentExecution }) => invoke<AppSnapshot>("hire_agent", { input }),
  setAgentMemory: (terminalId: string, content: string, expectedRevision: number) => invoke<AppSnapshot>("set_agent_memory", { terminalId, content, expectedRevision }),
  setAgentSoul: (terminalId: string, content: string, expectedRevision: number) => invoke<AppSnapshot>("set_agent_soul", { terminalId, content, expectedRevision }),
  approveAgentSoul: (terminalId: string, expectedRevision: number) => invoke<AppSnapshot>("approve_agent_soul", { terminalId, expectedRevision }),
  completeTraining: (terminalId: string, expectedRevision: number, expectedSoulRevision: number) => invoke<AppSnapshot>("complete_training", { terminalId, expectedRevision, expectedSoulRevision }),
  requestOnboarding: (terminalId: string, requestId: string) => invoke<AppSnapshot>("request_onboarding", { terminalId, requestId }),
  editAgent: (input: { terminalId: string; name: string; specialty: string; instructions: string; execution?: AgentExecution }) => invoke<AppSnapshot>("edit_agent", { input }),
  deleteManagerConversation: (input: { terminalId: string; taskCode: string | null; expectedThreadId: string | null }) => invoke<AppSnapshot>("delete_manager_conversation", { input }),
  selectManagerConversation: (terminalId: string, taskCode: string | null) => invoke<AppSnapshot>("select_manager_conversation", { terminalId, taskCode }),
  resetAgentContext: (terminalId: string, requestId: string) => invoke<AppSnapshot>("reset_agent_context", { input: { terminalId, requestId } }),
  assignAgent: (input: { terminalId: string; requestId: string; message: string; training?: boolean; freshContext?: boolean; taskCode?: string; stepIds?: string[] }) => invoke<AppSnapshot>("assign_agent", { input }),
  interruptAgent: (terminalId: string) => invoke<AppSnapshot>("interrupt_agent", { terminalId }),
  answerAgent: (terminalId: string, requestId: number | string, result: unknown, connectionId: string) => invoke<AppSnapshot>("answer_agent", { terminalId, requestId, result, connectionId }),
  snapshot: () => invoke<AppSnapshot>("get_snapshot"),
  createArea: (input: { name: string; rootPath: string; workspaceKind: "normal" | "team" }) =>
    invoke<AppSnapshot>("create_area", { input }),
  renameArea: (areaId: string, name: string) =>
    invoke<AppSnapshot>("rename_area", { areaId, name }),
  changeAreaRoot: (areaId: string, rootPath: string) =>
    invoke<AppSnapshot>("change_area_root", { areaId, rootPath }),
  moveArea: (areaId: string, direction: ReorderDirection) =>
    invoke<AppSnapshot>("move_area", { areaId, direction }),
  removeArea: (areaId: string) => invoke<AppSnapshot>("remove_area", { areaId }),
  createWorkspace: (input: { areaId: string; name: string; kind: "normal" | "team" }) =>
    invoke<AppSnapshot>("create_workspace", { input }),
  renameWorkspace: (workspaceId: string, name: string) =>
    invoke<AppSnapshot>("rename_workspace", { workspaceId, name }),
  moveWorkspace: (
    areaId: string,
    workspaceId: string,
    direction: ReorderDirection,
  ) => invoke<AppSnapshot>("move_workspace", { areaId, workspaceId, direction }),
  duplicateWorkspace: (workspaceId: string) =>
    invoke<AppSnapshot>("duplicate_workspace", { workspaceId }),
  removeWorkspace: (workspaceId: string) =>
    invoke<AppSnapshot>("remove_workspace", { workspaceId }),
  createTerminal: (input: CreateTerminal) =>
    invoke<AppSnapshot>("create_terminal", { input }),
  removeTerminal: (terminalId: string) =>
    invoke<AppSnapshot>("remove_terminal", { terminalId }),
  renameTerminal: (terminalId: string, name: string) =>
    invoke<AppSnapshot>("rename_terminal", { terminalId, name }),
  setTerminalLabel: (terminalId: string, label: string | null) =>
    invoke<AppSnapshot>("set_terminal_label", {
      input: { terminalId, label },
    }),
  setSplitRatio: (workspaceId: string, splitId: string, ratio: number) =>
    invoke<AppSnapshot>("set_split_ratio", {
      input: { workspaceId, splitId, ratio },
    }),
  startTerminal: (terminalId: string) =>
    invoke<AppSnapshot>("start_terminal", { terminalId }),
  stopTerminal: (terminalId: string) =>
    invoke<AppSnapshot>("stop_terminal", { terminalId }),
  writeTerminal: (terminalId: string, input: string) =>
    invoke<void>("write_terminal", { terminalId, input }),
  resizeTerminal: (terminalId: string, columns: number, rows: number) =>
    invoke<void>("resize_terminal", {
      input: { terminalId, columns, rows },
    }),
  createCustomProfile: (name: string) =>
    invoke<AppSnapshot>("create_custom_profile", { name }),
  configureCustomProfile: (
    profileId: string,
    executable: string,
    profileArguments: string[],
  ) =>
    invoke<AppSnapshot>("configure_custom_profile", {
      input: { profileId, executable, arguments: profileArguments },
    }),
  removeCustomProfile: (profileId: string) =>
    invoke<AppSnapshot>("remove_custom_profile", { profileId }),
};
