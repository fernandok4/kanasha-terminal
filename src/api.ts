import { invoke } from "@tauri-apps/api/core";
import type {
  AppSnapshot,
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
  snapshot: () => invoke<AppSnapshot>("get_snapshot"),
  createArea: (input: { name: string; rootPath: string }) =>
    invoke<AppSnapshot>("create_area", { input }),
  renameArea: (areaId: string, name: string) =>
    invoke<AppSnapshot>("rename_area", { areaId, name }),
  changeAreaRoot: (areaId: string, rootPath: string) =>
    invoke<AppSnapshot>("change_area_root", { areaId, rootPath }),
  moveArea: (areaId: string, direction: ReorderDirection) =>
    invoke<AppSnapshot>("move_area", { areaId, direction }),
  removeArea: (areaId: string) => invoke<AppSnapshot>("remove_area", { areaId }),
  createWorkspace: (input: { areaId: string; name: string }) =>
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
