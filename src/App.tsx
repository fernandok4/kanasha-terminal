import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { terminalApi } from "./api";
import { workspaceLayout } from "./workspaceLayout";
import { TerminalPanel } from "./components/TerminalPanel";
import { TaskMarkdown } from "./components/TaskMarkdown";
import { TeamView } from "./components/TeamView";
import { AreaInstructions } from "./components/AreaInstructions";
import { OfficeView } from "./components/OfficeView";
import type {
  AgentState,
  AgentStatus,
  AppSnapshot,
  Area,
  LayoutNode,
  ProfileView,
  SplitDirection,
  TaskState,
  TaskStatus,
  TerminalPanelModel,
  Workspace,
  WorkspaceKind,
} from "./types";
import { isTeamWorkspace } from "./types";
import "./App.css";

type MenuEntry = { label: string; description: string; action: () => void };
type ContextMenuState = { x: number; y: number; entries: MenuEntry[] } | null;
type WorkspaceView = "terminals" | "summary" | "team" | "office";
type TextDialogState =
  | { kind: "create-workspace"; areaId: string }
  | { kind: "rename-area"; areaId: string }
  | { kind: "rename-workspace"; workspaceId: string }
  | { kind: "rename-terminal"; terminalId: string }
  | { kind: "terminal-label"; terminalId: string };

const MIN_SPLIT_RATIO = 0.15;
const MAX_SPLIT_RATIO = 0.85;
const EMPTY_TASK: TaskState = { status: "not_started" };
const TASK_STATUS_LABELS: Record<TaskStatus, string> = {
  not_started: "Não iniciada",
  in_progress: "Em andamento",
  waiting: "Aguardando",
  blocked: "Bloqueada",
  done: "Concluída",
};
type AgentTone = AgentStatus | "offline";
const AGENT_STATUS_LABELS: Record<AgentTone, string> = {
  offline: "Desconectado",
  idle: "Disponível",
  working: "Trabalhando",
  waiting: "Aguardando",
  blocked: "Bloqueado",
  done: "Concluído",
};

function agentTone(
  terminalId: string,
  activeIds: Set<string>,
  agentStates: Record<string, AgentState>,
): AgentTone {
  if (!activeIds.has(terminalId)) return "offline";
  return agentStates[terminalId]?.status ?? "idle";
}

function workspaceAgentTone(
  workspace: Workspace,
  activeIds: Set<string>,
  agentStates: Record<string, AgentState>,
): AgentTone {
  const statuses = workspace.panels.map((panel) => agentTone(panel.id, activeIds, agentStates));
  if (statuses.includes("working")) return "working";
  if (statuses.includes("blocked")) return "blocked";
  if (statuses.includes("waiting")) return "waiting";
  if (statuses.includes("done")) return "done";
  if (statuses.includes("idle")) return "idle";
  return "offline";
}

function clampRatio(value: number) {
  return Math.min(MAX_SPLIT_RATIO, Math.max(MIN_SPLIT_RATIO, value));
}

function panelIdsInVisualOrder(node: LayoutNode): string[] {
  return node.kind === "panel"
    ? [node.panelId]
    : [...panelIdsInVisualOrder(node.first), ...panelIdsInVisualOrder(node.second)];
}

function errorMessage(error: unknown) {
  return typeof error === "string" ? error : "Não foi possível concluir a ação.";
}

function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [selectedAreaId, setSelectedAreaId] = useState<string | null>(null);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(null);
  const [selectedProfileId, setSelectedProfileId] = useState("shell");
  const [error, setError] = useState<string | null>(null);
  const [areaDialogOpen, setAreaDialogOpen] = useState(false);
  const [instructionsArea, setInstructionsArea] = useState<{ id: string; name: string } | null>(null);
  const [profileDialogOpen, setProfileDialogOpen] = useState(false);
  const [areaWorkspaceKind, setAreaWorkspaceKind] = useState<WorkspaceKind>("normal");
  const [newWorkspaceKind, setNewWorkspaceKind] = useState<WorkspaceKind>("normal");
  const [areaName, setAreaName] = useState("");
  const [areaPath, setAreaPath] = useState("");
  const [profileName, setProfileName] = useState("");
  const [profileExecutable, setProfileExecutable] = useState("");
  const [profileArguments, setProfileArguments] = useState("");
  const [configuredProfileId, setConfiguredProfileId] = useState<string | null>(null);
  const [textDialog, setTextDialog] = useState<TextDialogState | null>(null);
  const [textDialogValue, setTextDialogValue] = useState("");
  const [textDialogError, setTextDialogError] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenuState>(null);
  const [focusedPanelId, setFocusedPanelId] = useState<string | null>(null);
  const [maximizedPanelId, setMaximizedPanelId] = useState<string | null>(null);
  const [pendingSplitRatios, setPendingSplitRatios] = useState<Record<string, number>>({});
  const [workspaceView, setWorkspaceView] = useState<WorkspaceView>("terminals");
  const [resettingTerminalIds, setResettingTerminalIds] = useState<Set<string>>(new Set());
  const pendingResets = useRef(new Set<string>());
  const resetRequestIds = useRef(new Map<string, string>());
  const officeNavigation = useRef(0);
  const closingAfterConfirmation = useRef(false);

  const selectedArea = useMemo(
    () => snapshot?.areas.find((area) => area.id === selectedAreaId) ?? snapshot?.areas[0] ?? null,
    [snapshot, selectedAreaId],
  );
  const selectedWorkspace = useMemo(
    () => selectedArea?.workspaces.find((workspace) => workspace.id === selectedWorkspaceId) ?? selectedArea?.workspaces[0] ?? null,
    [selectedArea, selectedWorkspaceId],
  );
  useEffect(() => {
    if (workspaceView === "team" && (!selectedWorkspace || !isTeamWorkspace(selectedWorkspace))) setWorkspaceView("terminals");
  }, [selectedWorkspace, workspaceView]);
  const activeIds = useMemo(
    () => new Set(snapshot?.activeTerminalIds ?? []),
    [snapshot?.activeTerminalIds],
  );
  const agentStates = snapshot?.agentStates ?? {};
  const renderedWorkspaces = useMemo(
    () => (snapshot?.areas.flatMap((area) => area.workspaces) ?? [])
      .sort((left, right) => left.id.localeCompare(right.id)),
    [snapshot],
  );
  const visiblePanelIds = useMemo(
    () => {
      const layout = selectedWorkspace ? workspaceLayout(selectedWorkspace) : null;
      return layout ? panelIdsInVisualOrder(layout) : [];
    },
    [selectedWorkspace],
  );

  useEffect(() => {
    // A slow terminal start must not pull the user back after leaving this map.
    officeNavigation.current += 1;
  }, [selectedArea?.id, workspaceView]);

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("terminal-exit", () => {
      void refresh();
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("task-state-changed", () => {
      void refresh();
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const appWindow = getCurrentWindow();
    void appWindow
      .onCloseRequested(async (event) => {
        if (closingAfterConfirmation.current || activeIds.size === 0) return;
        event.preventDefault();
        if (
          !window.confirm(
            "Há processos ativos. Encerrá-los e fechar o KanashaTerminal?",
          )
        )
          return;
        try {
          for (const terminalId of activeIds) {
            applySnapshot(await terminalApi.stopTerminal(terminalId));
          }
          closingAfterConfirmation.current = true;
          await appWindow.close();
        } catch (actionError) {
          setError(errorMessage(actionError));
        }
      })
      .then((stopListening) => {
        if (disposed) stopListening();
        else unlisten = stopListening;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [activeIds]);

  useEffect(() => {
    if (!snapshot) return;
    setSelectedAreaId((previous) =>
      snapshot.areas.some((area) => area.id === previous)
        ? previous
        : (snapshot.areas[0]?.id ?? null),
    );
  }, [snapshot]);

  useEffect(() => {
    if (!selectedArea) {
      setSelectedWorkspaceId(null);
      return;
    }
    setSelectedWorkspaceId((previous) =>
      selectedArea.workspaces.some((workspace) => workspace.id === previous)
        ? previous
        : (selectedArea.workspaces[0]?.id ?? null),
    );
  }, [selectedArea]);

  useEffect(() => {
    if (!selectedArea || !selectedArea.workspaces.some(isTeamWorkspace)) return;
    let disposed = false;
    void terminalApi.prepareArea(selectedArea.id).then(() => {
      if (!disposed) void refresh();
    }).catch(e => { if (!disposed) setError(errorMessage(e)); });
    return () => { disposed = true; };
  }, [selectedArea?.id, selectedArea?.rootPath]);

  useEffect(() => {
    const onFocus = () => { void refresh(); };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  useEffect(() => {
    setFocusedPanelId((previous) =>
      previous && visiblePanelIds.includes(previous) ? previous : (visiblePanelIds[0] ?? null),
    );
    setMaximizedPanelId((previous) =>
      previous && visiblePanelIds.includes(previous) ? previous : null,
    );
  }, [selectedWorkspace?.id, visiblePanelIds]);

  useEffect(() => {
    setPendingSplitRatios({});
  }, [selectedWorkspace?.id]);

  useEffect(() => {
    const dismiss = () => setContextMenu(null);
    window.addEventListener("click", dismiss);
    return () => window.removeEventListener("click", dismiss);
  }, []);

  useEffect(() => {
    if (!areaDialogOpen && !profileDialogOpen && !textDialog) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setAreaDialogOpen(false);
      closeProfileDialog();
      closeTextDialog();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [areaDialogOpen, profileDialogOpen, textDialog]);

  async function refresh() {
    try {
      applySnapshot(await terminalApi.snapshot());
    } catch (actionError) {
      setError(errorMessage(actionError));
    }
  }

  function applySnapshot(next: AppSnapshot) {
    setSnapshot(next);
    setError(null);
    if (!next.profiles.some((profile) => profile.id === selectedProfileId)) {
      setSelectedProfileId("shell");
    }
  }

  async function run(action: Promise<AppSnapshot>) {
    try {
      applySnapshot(await action);
    } catch (actionError) {
      setError(errorMessage(actionError));
    }
  }

  async function newManagerConversation(terminalId: string) {
    if (pendingResets.current.has(terminalId)) return;
    pendingResets.current.add(terminalId);
    setResettingTerminalIds(new Set(pendingResets.current));
    const requestId = resetRequestIds.current.get(terminalId) ?? crypto.randomUUID();
    resetRequestIds.current.set(terminalId, requestId);
    try {
      applySnapshot(await terminalApi.resetAgentContext(terminalId, requestId));
      resetRequestIds.current.delete(terminalId);
    } catch (cause) {
      // Retain the request id so a retry cannot reset twice after an uncertain reply.
      setError(errorMessage(cause));
    } finally {
      pendingResets.current.delete(terminalId);
      setResettingTerminalIds(new Set(pendingResets.current));
    }
  }

  function showMenu(event: React.MouseEvent, entries: MenuEntry[]) {
    event.preventDefault();
    setContextMenu({ x: event.clientX, y: event.clientY, entries });
  }

  async function openOfficeTerminal(workspaceId: string, terminalId: string) {
    // Reuse the existing session; the terminal tree stays mounted behind the map.
    const navigation = officeNavigation.current;
    if (!activeIds.has(terminalId) || snapshot?.memberStates?.[terminalId]?.connected === false) {
      applySnapshot(await terminalApi.startTerminal(terminalId));
    }
    if (navigation !== officeNavigation.current) return;
    setSelectedWorkspaceId(workspaceId);
    setFocusedPanelId(terminalId);
    setMaximizedPanelId(terminalId);
    setWorkspaceView("terminals");
  }

  function showMenuFromButton(event: React.MouseEvent<HTMLButtonElement>, entries: MenuEntry[]) {
    event.stopPropagation();
    const bounds = event.currentTarget.getBoundingClientRect();
    setContextMenu({ x: bounds.left, y: bounds.bottom + 4, entries });
  }

  async function chooseFolder(setPath: (value: string) => void) {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Selecionar pasta-raiz",
    });
    if (typeof selected === "string") setPath(selected);
  }

  async function createArea(event: React.FormEvent) {
    event.preventDefault();
    await run(terminalApi.createArea({ name: areaName, rootPath: areaPath, workspaceKind: areaWorkspaceKind }));
    setAreaDialogOpen(false);
    setAreaName("");
    setAreaPath("");
    setAreaWorkspaceKind("normal");
  }

  function openCreateWorkspaceDialog() {
    if (!selectedArea) return;
    setNewWorkspaceKind("normal");
    openTextDialog({ kind: "create-workspace", areaId: selectedArea.id }, "Workspace novo");
  }

  function openTextDialog(dialog: TextDialogState, value: string) {
    setTextDialog(dialog);
    setTextDialogValue(value);
    setTextDialogError(null);
  }

  function closeTextDialog() {
    setTextDialog(null);
    setTextDialogValue("");
    setTextDialogError(null);
  }

  async function saveTextDialog(event: React.FormEvent) {
    event.preventDefault();
    if (!textDialog) return;

    const value = textDialogValue.trim();
    const isLabel = textDialog.kind === "terminal-label";
    if (!isLabel && !value) {
      setTextDialogError("Informe um nome para continuar.");
      return;
    }
    if (value.includes("\0") || (!isLabel && value.length > 120)) {
      setTextDialogError("Informe um nome válido com até 120 caracteres.");
      return;
    }
    if (isLabel && (value.includes("\r") || value.includes("\n") || value.length > 32)) {
      setTextDialogError("Informe uma etiqueta com até 32 caracteres em uma linha.");
      return;
    }

    try {
      let next: AppSnapshot;
      switch (textDialog.kind) {
        case "create-workspace":
          next = await terminalApi.createWorkspace({ areaId: textDialog.areaId, name: value, kind: newWorkspaceKind });
          {
            const existing = new Set(snapshot?.areas.flatMap(a => a.workspaces).map(w => w.id));
            const created = next.areas.find(a => a.id === textDialog.areaId)?.workspaces.find(w => !existing.has(w.id));
            if (created) { setSelectedWorkspaceId(created.id); setWorkspaceView(isTeamWorkspace(created) ? "team" : "terminals"); }
          }
          break;
        case "rename-area":
          next = await terminalApi.renameArea(textDialog.areaId, value);
          break;
        case "rename-workspace":
          next = await terminalApi.renameWorkspace(textDialog.workspaceId, value);
          break;
        case "rename-terminal":
          next = await terminalApi.renameTerminal(textDialog.terminalId, value);
          break;
        case "terminal-label":
          next = await terminalApi.setTerminalLabel(textDialog.terminalId, value || null);
          break;
      }
      applySnapshot(next);
      closeTextDialog();
    } catch (actionError) {
      setTextDialogError(errorMessage(actionError));
    }
  }

  function focusAdjacentPanel(offset: -1 | 1) {
    if (visiblePanelIds.length === 0) return;
    const currentIndex = focusedPanelId ? visiblePanelIds.indexOf(focusedPanelId) : -1;
    const nextIndex =
      currentIndex < 0
        ? 0
        : (currentIndex + offset + visiblePanelIds.length) % visiblePanelIds.length;
    setMaximizedPanelId(null);
    setFocusedPanelId(visiblePanelIds[nextIndex]);
  }

  function openNewProfileDialog() {
    setConfiguredProfileId(null);
    setProfileName("");
    setProfileExecutable("");
    setProfileArguments("");
    setProfileDialogOpen(true);
  }

  function openProfileConfiguration(profile: ProfileView) {
    setConfiguredProfileId(profile.id);
    setProfileName(profile.name);
    setProfileExecutable("");
    setProfileArguments("");
    setProfileDialogOpen(true);
  }

  function closeProfileDialog() {
    setProfileDialogOpen(false);
    setConfiguredProfileId(null);
    setProfileName("");
    setProfileExecutable("");
    setProfileArguments("");
  }

  async function addTerminal(targetPanelId?: string, direction?: SplitDirection) {
    if (!selectedWorkspace || !snapshot) return;
    const profile = snapshot.profiles.find((item) => item.id === selectedProfileId);
    if (!profile?.available) {
      setError("O perfil selecionado não está disponível ou ainda não foi configurado.");
      return;
    }
    try {
      const previousPanelIds = new Set(selectedWorkspace.panels.map((panel) => panel.id));
      const created = await terminalApi.createTerminal({
        workspaceId: selectedWorkspace.id,
        profileId: selectedProfileId,
        targetPanelId,
        direction,
      });
      const createdWorkspace = created.areas
        .flatMap((area) => area.workspaces)
        .find((workspace) => workspace.id === selectedWorkspace.id);
      const panel = createdWorkspace?.panels.find((item) => !previousPanelIds.has(item.id));
      if (!panel) throw new Error("Não foi possível identificar o painel criado.");
      applySnapshot(created);
      setFocusedPanelId(panel.id);
      setMaximizedPanelId(null);
      applySnapshot(await terminalApi.startTerminal(panel.id));
    } catch (actionError) {
      setError(errorMessage(actionError));
    }
  }

  async function persistSplitRatio(splitId: string, ratio: number) {
    if (!selectedWorkspace) return;
    try {
      applySnapshot(
        await terminalApi.setSplitRatio(selectedWorkspace.id, splitId, clampRatio(ratio)),
      );
    } catch (actionError) {
      setError(errorMessage(actionError));
    } finally {
      setPendingSplitRatios((current) => {
        const { [splitId]: _discarded, ...remaining } = current;
        return remaining;
      });
    }
  }

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      const target = event.target;
      const terminalHasFocus =
        target instanceof Element && Boolean(target.closest(".terminal-canvas"));
      const editingText =
        target instanceof Element &&
        Boolean(target.closest("input, textarea, select, [contenteditable='true']"));
      if ((areaDialogOpen || profileDialogOpen || textDialog) || (editingText && !terminalHasFocus)) return;
      if (
        event.altKey &&
        ["ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown"].includes(event.key)
      ) {
        event.preventDefault();
        focusAdjacentPanel(event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1);
        return;
      }
      if (terminalHasFocus) {
        if (event.shiftKey && event.key.toLowerCase() === "n") {
          event.preventDefault();
          void addTerminal();
        }
        return;
      }
      if (event.shiftKey && event.key.toLowerCase() === "a") {
        event.preventDefault();
        setAreaDialogOpen(true);
      } else if (event.shiftKey && event.key.toLowerCase() === "w") {
        event.preventDefault();
        openCreateWorkspaceDialog();
      } else if (event.key === ",") {
        event.preventDefault();
        openNewProfileDialog();
      } else if (event.shiftKey && event.key.toLowerCase() === "n") {
        event.preventDefault();
        void addTerminal();
      } else if (!event.shiftKey && event.key.toLowerCase() === "n") {
        event.preventDefault();
        void addTerminal();
      }
    };
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [selectedWorkspace, selectedArea, selectedProfileId, areaDialogOpen, profileDialogOpen, textDialog, focusedPanelId, visiblePanelIds]);

  async function stopAndRemoveTerminal(terminalId: string) {
    const isActive = activeIds.has(terminalId);
    if (isActive && !window.confirm("Este processo será encerrado. Deseja continuar?")) return;
    try {
      if (isActive) await terminalApi.stopTerminal(terminalId);
      applySnapshot(await terminalApi.removeTerminal(terminalId));
    } catch (actionError) {
      setError(errorMessage(actionError));
    }
  }

  async function stopThenRemoveWorkspace(workspace: Workspace) {
    const activePanels = workspace.panels.filter((panel) => activeIds.has(panel.id));
    if (
      activePanels.length > 0 &&
      !window.confirm("Os processos ativos deste workspace serão encerrados. Deseja continuar?")
    ) return;
    try {
      for (const panel of activePanels) await terminalApi.stopTerminal(panel.id);
      applySnapshot(await terminalApi.removeWorkspace(workspace.id));
    } catch (actionError) {
      setError(errorMessage(actionError));
    }
  }

  async function stopThenRemoveArea(area: Area) {
    const activePanels = area.workspaces.flatMap((workspace) =>
      workspace.panels.filter((panel) => activeIds.has(panel.id)),
    );
    if (
      activePanels.length > 0 &&
      !window.confirm("Os processos ativos desta área serão encerrados. Deseja continuar?")
    ) return;
    try {
      for (const panel of activePanels) await terminalApi.stopTerminal(panel.id);
      applySnapshot(await terminalApi.removeArea(area.id));
    } catch (actionError) {
      setError(errorMessage(actionError));
    }
  }

  async function createProfile(event: React.FormEvent) {
    event.preventDefault();
    try {
      const argumentsList = profileArguments
        .split("\n")
        .map((argument) => argument.trim())
        .filter(Boolean);
      if (configuredProfileId) {
        applySnapshot(
          await terminalApi.configureCustomProfile(
            configuredProfileId,
            profileExecutable,
            argumentsList,
          ),
        );
        setSelectedProfileId(configuredProfileId);
      } else {
        const created = await terminalApi.createCustomProfile(profileName);
        const profile = [...created.profiles]
          .reverse()
          .find((item) => !item.builtIn && item.name === profileName.trim());
        if (!profile) throw new Error("O perfil não foi localizado após sua criação.");
        applySnapshot(
          await terminalApi.configureCustomProfile(profile.id, profileExecutable, argumentsList),
        );
        setSelectedProfileId(profile.id);
      }
      closeProfileDialog();
    } catch (actionError) {
      setError(errorMessage(actionError));
    }
  }

  function areaMenu(area: Area): MenuEntry[] {
    return [
      {
        label: "Renomear área",
        description: "Altera apenas o nome exibido no KanashaTerminal.",
        action: () => openTextDialog({ kind: "rename-area", areaId: area.id }, area.name),
      },
      {
        label: "Alterar pasta-raiz",
        description: "Troca a pasta associada à área, sem apagar arquivos.",
        action: async () => {
          const selected = await open({ directory: true, multiple: false, title: "Nova pasta-raiz" });
          if (typeof selected === "string") await run(terminalApi.changeAreaRoot(area.id, selected));
        },
      },
      { label: "Mover para cima", description: "Reordena a área na barra lateral.", action: () => void run(terminalApi.moveArea(area.id, "earlier")) },
      { label: "Mover para baixo", description: "Reordena a área na barra lateral.", action: () => void run(terminalApi.moveArea(area.id, "later")) },
      { label: "Remover da aplicação", description: "Remove somente a configuração; a pasta nunca é apagada.", action: () => void stopThenRemoveArea(area) },
    ];
  }

  function profilesMenu(): MenuEntry[] {
    const customProfiles = (snapshot?.profiles ?? []).filter((profile) => !profile.builtIn);
    return [
      {
        label: "Criar perfil local",
        description: "Cria um perfil para selecionar no topo desta tela.",
        action: openNewProfileDialog,
      },
      ...customProfiles.flatMap((profile) => [
        {
          label: `Configurar ${profile.name}`,
          description: "Informa executável e argumentos apenas para a sessão atual.",
          action: () => openProfileConfiguration(profile),
        },
        {
          label: `Remover ${profile.name}`,
          description: "Remove o metadado local; não remove arquivos nem comandos.",
          action: () => {
            if (window.confirm(`Remover o perfil ${profile.name}?`)) {
              void run(terminalApi.removeCustomProfile(profile.id));
            }
          },
        },
      ]),
    ];
  }

  function terminalMenu(terminal: TerminalPanelModel): MenuEntry[] {
    const terminalId = terminal.id;
    return [
      {
        label: "Renomear terminal",
        description: "Define o nome exibido deste painel, sem alterar o processo.",
        action: () => openTextDialog({ kind: "rename-terminal", terminalId }, terminal.title),
      },
      {
        label: terminal.label ? "Alterar etiqueta" : "Definir etiqueta",
        description: "Exibe uma marca curta para identificar este terminal mais rapidamente.",
        action: () => openTextDialog({ kind: "terminal-label", terminalId }, terminal.label ?? ""),
      },
      {
        label: activeIds.has(terminalId) ? "Encerrar processo" : "Iniciar terminal",
        description: activeIds.has(terminalId) ? "Encerra somente este processo." : "Inicia o perfil local deste painel.",
        action: () => {
          if (activeIds.has(terminalId)) {
            if (window.confirm("Encerrar este processo?")) void run(terminalApi.stopTerminal(terminalId));
          } else {
            void run(terminalApi.startTerminal(terminalId));
          }
        },
      },
      { label: "Dividir à direita", description: "Cria um painel ao lado usando o perfil selecionado.", action: () => void addTerminal(terminalId, "vertical") },
      { label: "Dividir abaixo", description: "Cria um painel abaixo usando o perfil selecionado.", action: () => void addTerminal(terminalId, "horizontal") },
      { label: "Fechar painel", description: "Remove o painel e pede confirmação se o processo estiver ativo.", action: () => void stopAndRemoveTerminal(terminalId) },
    ];
  }

  function workspaceMenu(workspace: Workspace): MenuEntry[] {
    return [
      {
        label: "Renomear",
        description: "Altera somente o nome desta tela.",
        action: () => openTextDialog({ kind: "rename-workspace", workspaceId: workspace.id }, workspace.name),
      },
      {
        label: "Duplicar",
        description: "Duplica a estrutura de painéis sem copiar processos.",
        action: () => void run(terminalApi.duplicateWorkspace(workspace.id)),
      },
      {
        label: "Mover para esquerda",
        description: "Reordena a aba deste workspace.",
        action: () => {
          if (selectedArea) void run(terminalApi.moveWorkspace(selectedArea.id, workspace.id, "earlier"));
        },
      },
      {
        label: "Mover para direita",
        description: "Reordena a aba deste workspace.",
        action: () => {
          if (selectedArea) void run(terminalApi.moveWorkspace(selectedArea.id, workspace.id, "later"));
        },
      },
      {
        label: "Fechar workspace",
        description: "Remove a configuração e pede confirmação se houver processo ativo.",
        action: () => void stopThenRemoveWorkspace(workspace),
      },
    ];
  }

  if (!snapshot) return <main className="loading-screen">Carregando KanashaTerminal…</main>;

  return (
    <main className="app-shell">
      <aside className="areas-sidebar" aria-label="Áreas de trabalho">
        <div className="brand"><span className="brand-mark" aria-hidden="true">KT</span><div><strong>KanashaTerminal</strong><small>Terminais locais, sem nuvem</small></div></div>
        <div className="sidebar-heading"><span>Áreas de trabalho</span><button title="Criar nova área" aria-label="Criar nova área" onClick={() => setAreaDialogOpen(true)}>+</button></div>
        <nav className="area-list">
          {snapshot.areas.map((area) => (
            <div className={`area-row ${area.id === selectedArea?.id ? "selected" : ""}`} key={area.id}>
              <button className="area-item" aria-current={area.id === selectedArea?.id ? "page" : undefined} title={`Selecionar área ${area.name}`} onClick={() => setSelectedAreaId(area.id)} onContextMenu={(event) => showMenu(event, areaMenu(area))}>
                <span aria-hidden="true">▰</span><span>{area.name}</span>
              </button>
              <button className="context-trigger" title={`Ações da área ${area.name}`} aria-label={`Ações da área ${area.name}`} aria-haspopup="menu" onClick={(event) => showMenuFromButton(event, areaMenu(area))}>⋯</button>
            </div>
          ))}
        </nav>
      </aside>

      <section className="workspace-shell">
        {selectedArea ? (
          <header className="workspace-tabs" aria-label="Workspaces">
            <span className="area-path" title={selectedArea.rootPath}>{selectedArea.name}</span>
            <button className="toolbar-secondary" onClick={() => setInstructionsArea({ id: selectedArea.id, name: selectedArea.name })}>Instruções da Área</button>
            {selectedArea.workspaces.some(isTeamWorkspace) ? <button className="office-toggle" aria-pressed={workspaceView === "office"} title="Ver escritório da Área de Trabalho" onClick={() => setWorkspaceView(current => current === "office" ? "terminals" : "office")}>Escritório</button> : null}
            <div className="tabs" role="tablist" aria-label={`Workspaces de ${selectedArea.name}`}>
              {selectedArea.workspaces.map((workspace) => {
                const tone = workspaceAgentTone(workspace, activeIds, agentStates);
                return <div className={`tab-group agent-${tone}`} data-agent-status={tone} key={workspace.id}>
                  <button role="tab" aria-selected={workspace.id === selectedWorkspace?.id} className={workspace.id === selectedWorkspace?.id ? "tab selected" : "tab"} title={`${workspace.name}: ${AGENT_STATUS_LABELS[tone]}`} onClick={() => setSelectedWorkspaceId(workspace.id)} onContextMenu={(event) => showMenu(event, workspaceMenu(workspace))}><span className="workspace-agent-dot" aria-hidden="true" />{workspace.name}</button>
                  <button className="context-trigger tab-context-trigger" title={`Ações do workspace ${workspace.name}`} aria-label={`Ações do workspace ${workspace.name}`} aria-haspopup="menu" onClick={(event) => showMenuFromButton(event, workspaceMenu(workspace))}>⋯</button>
                </div>;
              })}
              <button className="tab add-tab" title="Criar novo workspace" aria-label="Criar novo workspace" onClick={openCreateWorkspaceDialog}>+</button>
            </div>
          </header>
        ) : snapshot.areas.length === 0 ? (
          <section className="empty-workspace onboarding"><p className="eyebrow">Primeiro uso</p><h1>Organize seus agentes por área e workspace.</h1><p>Uma Área aponta para uma pasta-raiz local. Dentro dela você cria workspaces e abre terminais em paralelo.</p><button title="Criar primeira área" onClick={() => setAreaDialogOpen(true)}>Criar primeira área</button><button className="onboarding-secondary" title="Gerenciar perfis locais" aria-label="Gerenciar perfis" aria-haspopup="menu" onClick={(event) => showMenuFromButton(event, profilesMenu())}>Gerenciar perfis</button></section>
        ) : null}
        {selectedWorkspace ? <div className="workspace-toolbar">
          <label htmlFor="profile-select">Perfil</label>
          <select id="profile-select" value={selectedProfileId} onChange={(event) => setSelectedProfileId(event.target.value)}>{snapshot.profiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}{profile.available ? "" : " — indisponível"}</option>)}</select>
          <button className="toolbar-secondary" title="Gerenciar perfis locais" aria-label="Gerenciar perfis" aria-haspopup="menu" onClick={(event) => showMenuFromButton(event, profilesMenu())}>Perfis</button>
          <button title="Adicionar novo terminal" onClick={() => void addTerminal()}>+ Terminal</button>
          {isTeamWorkspace(selectedWorkspace) ? <button className={`toolbar-secondary ${workspaceView === "team" ? "is-active" : ""}`} aria-pressed={workspaceView === "team"} onClick={() => setWorkspaceView(current => current === "team" ? "terminals" : "team")}>{workspaceView === "team" ? "Voltar aos terminais" : "Equipe"}</button> : null}
          <button className={`toolbar-secondary summary-toggle ${workspaceView === "summary" ? "is-active" : ""}`} title={workspaceView === "summary" ? "Voltar aos terminais" : "Ver resumo da tarefa"} aria-pressed={workspaceView === "summary"} onClick={() => setWorkspaceView((current) => current === "summary" ? "terminals" : "summary")}>{workspaceView === "summary" ? "Terminais" : "Resumo"}</button>
          <span className={`task-status-dot agent-${workspaceAgentTone(selectedWorkspace, activeIds, agentStates)}`} title={`Agentes: ${AGENT_STATUS_LABELS[workspaceAgentTone(selectedWorkspace, activeIds, agentStates)]}`} aria-hidden="true" />
          <span className="privacy-note">{workspaceView === "summary" ? "Resumo salvo somente neste dispositivo." : selectedWorkspace.panels.some(p => p.member) ? "Equipe local · sessões e histórico no Codex" : "Nada digitado ou exibido no terminal é salvo."}</span>
        </div> : null}
        <section
          className={`terminal-stage ${maximizedPanelId ? "is-maximized" : ""} ${workspaceView !== "terminals" ? "is-hidden" : ""}`}
          aria-label={selectedWorkspace ? `Terminais de ${selectedWorkspace.name}` : "Terminais"}
          aria-hidden={workspaceView !== "terminals" || !selectedWorkspace}
          hidden={!selectedWorkspace}
        >
          {renderedWorkspaces.map((workspace) => {
            const isSelected = workspace.id === selectedWorkspace?.id;
            const layout = workspaceLayout(workspace);
            return <div className={`workspace-terminal-layout ${isSelected ? "" : "is-hidden"}`} key={workspace.id} aria-hidden={!isSelected}>
              {layout ? <LayoutTree memberStates={snapshot.memberStates} resettingTerminalIds={resettingTerminalIds} onNewConversation={id => void newManagerConversation(id)} node={layout} automatic={workspace.panels.some(panel => panel.member?.manager)} panels={workspace.panels} activeIds={activeIds} agentStates={agentStates} focusedPanelId={focusedPanelId} maximizedPanelId={maximizedPanelId} pendingSplitRatios={pendingSplitRatios} onStart={(id) => void run(terminalApi.startTerminal(id))} onStop={(id) => { if (window.confirm("Encerrar este processo?")) void run(terminalApi.stopTerminal(id)); }} onClose={(id) => void stopAndRemoveTerminal(id)} onSplit={(id, direction) => void addTerminal(id, direction)} onFocus={setFocusedPanelId} onToggleMaximize={(id) => { setFocusedPanelId(id); setMaximizedPanelId((current) => current === id ? null : id); }} onRatioChange={(splitId, ratio) => setPendingSplitRatios((current) => ({ ...current, [splitId]: ratio }))} onRatioCommit={(splitId, ratio) => void persistSplitRatio(splitId, ratio)} onContextMenu={(event, id) => { const panel = workspace.panels.find((item) => item.id === id); if (panel) showMenu(event, terminalMenu(panel)); }} /> : <div className="empty-workspace"><h1>{workspace.name}</h1><p>Abra o primeiro terminal para começar. Ele iniciará na pasta-raiz da Área.</p><button title="Abrir primeiro terminal" onClick={() => void addTerminal()}>Abrir terminal</button></div>}
            </div>;
          })}
        </section>
        {selectedWorkspace ? <TaskSummary workspace={selectedWorkspace} activeIds={activeIds} agentStates={agentStates} visible={workspaceView === "summary"} /> : null}
        {selectedWorkspace && isTeamWorkspace(selectedWorkspace) && workspaceView === "team" ? <TeamView key={selectedWorkspace.id} workspace={selectedWorkspace} snapshot={snapshot} onSnapshot={applySnapshot} onOpen={id => { setFocusedPanelId(id); setMaximizedPanelId(id); setWorkspaceView("terminals"); }} /> : null}
        {selectedArea && workspaceView === "office" ? <OfficeView key={selectedArea.id} area={selectedArea} snapshot={snapshot} onOpen={openOfficeTerminal} onTeam={id => { setSelectedWorkspaceId(id); setWorkspaceView("team"); }} onClose={() => setWorkspaceView("terminals")} /> : null}
      </section>

      {error ? <div className="error-toast" role="alert">{error}<button title="Fechar mensagem" onClick={() => setError(null)}>×</button></div> : null}
      {contextMenu ? <ContextMenu menu={contextMenu} onClose={() => setContextMenu(null)} /> : null}
      {areaDialogOpen ? <AreaDialog workspaceKind={areaWorkspaceKind} onWorkspaceKind={setAreaWorkspaceKind} name={areaName} path={areaPath} onName={setAreaName} onPath={setAreaPath} onChoose={() => void chooseFolder(setAreaPath)} onClose={() => setAreaDialogOpen(false)} onSubmit={createArea} /> : null}
      {instructionsArea ? <AreaInstructions key={instructionsArea.id} areaId={instructionsArea.id} areaName={instructionsArea.name} onClose={() => setInstructionsArea(null)} onSaved={() => void refresh()} /> : null}
      {profileDialogOpen ? <ProfileDialog editing={configuredProfileId !== null} name={profileName} executable={profileExecutable} argumentsText={profileArguments} onName={setProfileName} onExecutable={setProfileExecutable} onArguments={setProfileArguments} onClose={closeProfileDialog} onSubmit={createProfile} /> : null}
      {textDialog ? <TextEditDialog workspaceKind={newWorkspaceKind} onWorkspaceKind={setNewWorkspaceKind} dialog={textDialog} value={textDialogValue} error={textDialogError} onValue={setTextDialogValue} onClose={closeTextDialog} onSubmit={saveTextDialog} /> : null}
    </main>
  );
}

function TaskSummary({ workspace, activeIds, agentStates, visible }: { workspace: Workspace; activeIds: Set<string>; agentStates: Record<string, AgentState>; visible: boolean }) {
  const task = workspace.task ?? EMPTY_TASK;
  const hasContext = Boolean(task.summary || task.steps?.length || task.current || task.next || task.blocker);
  const agents = workspace.panels.map((panel) => {
    const tone = agentTone(panel.id, activeIds, agentStates);
    return { panel, tone, state: agentStates[panel.id] };
  });
  const workingAgents = agents.filter((agent) => agent.tone === "working").length;
  const updatedAt = task.updatedAt
    ? new Intl.DateTimeFormat("pt-BR", {
        dateStyle: "short",
        timeStyle: "short",
      }).format(new Date(task.updatedAt * 1000))
    : "Ainda não atualizado";

  return (
    <section className={`task-summary-stage ${visible ? "" : "is-hidden"}`} aria-label={`Resumo da tarefa ${workspace.name}`} aria-hidden={!visible}>
      <div className="task-summary-content">
        <header className="task-summary-header">
          <div>
            <p className="eyebrow">Contexto do workspace</p>
            <h1>{workspace.name}</h1>
          </div>
          <span className={`task-status status-${task.status}`}>{TASK_STATUS_LABELS[task.status]}</span>
        </header>

        {hasContext ? (
          <div className="task-summary-grid">
            <article className="task-summary-card task-summary-overview">
              <span>Resumo atual</span>
              <TaskMarkdown content={task.summary || "O agente ainda não registrou um resumo."} />
              {task.steps?.length ? <div className="task-summary-steps">
                <h2>Etapas</h2>
                <ol aria-label="Etapas da tarefa">
                  {task.steps.map((step, index) => <li key={step.id}>
                    <span className="step-number" aria-hidden="true">{index + 1}</span>
                    <span className="step-title">{step.title}</span>
                    <span className={`task-status status-${step.status}`}>{TASK_STATUS_LABELS[step.status]}</span>
                  </li>)}
                </ol>
              </div> : null}
            </article>
            <article className="task-summary-card">
              <span>Em execução</span>
              <p>{task.current || "Nenhuma etapa informada."}</p>
            </article>
            <article className="task-summary-card">
              <span>Próximo passo</span>
              <p>{task.next || "Nenhum próximo passo informado."}</p>
            </article>
            {task.blocker ? <article className="task-summary-card task-summary-blocker">
              <span>Bloqueio</span>
              <p>{task.blocker}</p>
            </article> : null}
          </div>
        ) : (
          <div className="task-summary-empty">
            <span aria-hidden="true">◎</span>
            <h2>Aguardando contexto da tarefa</h2>
            <p>O agente publicará o objetivo de cada tarefa, o que foi implementado, os testes, as pendências e os links de acompanhamento.</p>
          </div>
        )}

        <section className="agent-summary" aria-label="Status dos agentes">
          <header>
            <div>
              <p className="eyebrow">Agentes deste workspace</p>
              <h2>Status dos terminais</h2>
            </div>
            <span>{workingAgents} trabalhando</span>
          </header>
          {agents.length ? <div className="agent-summary-grid">
            {agents.map(({ panel, tone, state }) => (
              <article className={`agent-card agent-${tone}`} data-agent-status={tone} key={panel.id}>
                <div className="agent-card-heading">
                  <span className="agent-card-dot" aria-hidden="true" />
                  <strong>{panel.title}</strong>
                  <span className="agent-status-label">{AGENT_STATUS_LABELS[tone]}</span>
                </div>
                <p>{state?.current || (tone === "offline" ? "Terminal parado." : "Aguardando o primeiro reporte do agente.")}</p>
                <small>{panel.label || panel.profileId}</small>
              </article>
            ))}
          </div> : <p className="agent-summary-empty">Nenhum terminal foi criado neste Workspace.</p>}
        </section>

        <footer className="task-summary-footer">
          <span>Atualizado: {updatedAt}</span>
          {task.revision ? <span>Revisão {task.revision}</span> : null}
        </footer>
      </div>
    </section>
  );
}

type LayoutTreeProps = {
  automatic: boolean;
  node: LayoutNode;
  panels: TerminalPanelModel[];
  activeIds: Set<string>;
  agentStates: Record<string, AgentState>;
  focusedPanelId: string | null;
  maximizedPanelId: string | null;
  pendingSplitRatios: Record<string, number>;
  onStart: (id: string) => void;
  onStop: (id: string) => void;
  onNewConversation: (id: string) => void;
  resettingTerminalIds: Set<string>;
  memberStates: AppSnapshot["memberStates"];
  onClose: (id: string) => void;
  onSplit: (id: string, direction: SplitDirection) => void;
  onFocus: (id: string) => void;
  onToggleMaximize: (id: string) => void;
  onRatioChange: (splitId: string, ratio: number) => void;
  onRatioCommit: (splitId: string, ratio: number) => void;
  onContextMenu: (event: React.MouseEvent, id: string) => void;
};

// Positions are affine lengths: a percentage of the workspace plus a pixel
// offset for the dividers. Only geometry follows the split tree; terminal DOM
// nodes remain siblings with stable keys throughout splits and collapses.
type LayoutLength = { percent: number; pixels: number };
type LayoutRect = { left: LayoutLength; top: LayoutLength; width: LayoutLength; height: LayoutLength };
const FULL_LAYOUT_RECT: LayoutRect = {
  left: { percent: 0, pixels: 0 }, top: { percent: 0, pixels: 0 },
  width: { percent: 100, pixels: 0 }, height: { percent: 100, pixels: 0 },
};
const DIVIDER_SIZE = 9;

function rectStyle(rect: LayoutRect): CSSProperties {
  const length = ({ percent, pixels }: LayoutLength) => `calc(${percent}% ${pixels < 0 ? "-" : "+"} ${Math.abs(pixels)}px)`;
  return { position: "absolute", left: length(rect.left), top: length(rect.top), width: length(rect.width), height: length(rect.height) };
}

function LayoutTree({ memberStates, resettingTerminalIds, onNewConversation, node, automatic, panels, activeIds, agentStates, focusedPanelId, maximizedPanelId, pendingSplitRatios, onStart, onStop, onClose, onSplit, onFocus, onToggleMaximize, onRatioChange, onRatioCommit, onContextMenu }: LayoutTreeProps) {
  const positions = new Map<string, CSSProperties>();
  const dividers: { node: Extract<LayoutNode, { kind: "split" }>; ratio: number; region: CSSProperties; style: CSSProperties }[] = [];
  const visit = (current: LayoutNode, rect: LayoutRect) => {
    if (current.kind === "panel") {
      positions.set(current.panelId, rectStyle(rect));
      return;
    }
    const ratio = automatic ? current.ratio : clampRatio(pendingSplitRatios[current.id] ?? current.ratio);
    const vertical = current.direction === "vertical";
    const axis = vertical ? "left" : "top";
    const size = vertical ? "width" : "height";
    const available = { ...rect[size], pixels: rect[size].pixels - DIVIDER_SIZE };
    const firstSize = { percent: available.percent * ratio, pixels: available.pixels * ratio };
    const secondSize = { percent: available.percent * (1 - ratio), pixels: available.pixels * (1 - ratio) };
    const secondStart = { percent: rect[axis].percent + firstSize.percent, pixels: rect[axis].pixels + firstSize.pixels + DIVIDER_SIZE };
    dividers.push({ node: current, ratio, region: rectStyle(rect), style: {
      position: "absolute", [axis]: `calc(${ratio * 100}% - ${DIVIDER_SIZE * ratio}px)`,
      [size]: DIVIDER_SIZE, [vertical ? "top" : "left"]: 0, [vertical ? "height" : "width"]: "100%",
    } });
    visit(current.first, { ...rect, [size]: firstSize });
    visit(current.second, { ...rect, [axis]: secondStart, [size]: secondSize });
  };
  visit(node, FULL_LAYOUT_RECT);
  const maximized = maximizedPanelId && positions.has(maximizedPanelId) ? maximizedPanelId : null;
  return <>
    {[...panels].sort((a, b) => a.id.localeCompare(b.id)).map((panel) => {
      const visible = positions.has(panel.id) && (!maximized || maximized === panel.id);
      const style = visible ? (maximized ? rectStyle(FULL_LAYOUT_RECT) : positions.get(panel.id)) : { display: "none" };
      const runtime = memberStates?.[panel.id];
      const newConversationDisabled = resettingTerminalIds.has(panel.id) || !!panel.member?.paused || !!runtime?.turnId || !!runtime?.request || !!panel.member?.jobs.some(job => job.status === "running" || job.status === "uncertain");
      return <TerminalPanel onNewConversation={onNewConversation} newConversationDisabled={newConversationDisabled} key={panel.id} style={style} panel={panel} active={activeIds.has(panel.id)} agentStatus={agentStates[panel.id]?.status} focused={focusedPanelId === panel.id} maximized={maximized === panel.id} onStart={onStart} onStop={onStop} onClose={onClose} onSplit={onSplit} onFocus={onFocus} onToggleMaximize={onToggleMaximize} onContextMenu={onContextMenu} />;
    })}
    {!maximized && !automatic && dividers.map(({ node: split, ratio, region, style }) => <div key={split.id} className="terminal-divider-region" style={region}>
      <SplitDivider splitId={split.id} direction={split.direction} ratio={ratio} style={style} onChange={onRatioChange} onCommit={onRatioCommit} />
    </div>)}
  </>;
}

function SplitDivider({ splitId, direction, ratio, style, onChange, onCommit }: { style: CSSProperties; splitId: string; direction: SplitDirection; ratio: number; onChange: (splitId: string, ratio: number) => void; onCommit: (splitId: string, ratio: number) => void }) {
  const latestRatio = useRef(ratio);

  useEffect(() => {
    latestRatio.current = ratio;
  }, [ratio]);

  const update = (nextRatio: number) => {
    const next = clampRatio(nextRatio);
    latestRatio.current = next;
    onChange(splitId, next);
  };
  const vertical = direction === "vertical";

  const adjustFromKeyboard = (event: React.KeyboardEvent<HTMLDivElement>) => {
    let next: number | null = null;
    if (event.key === "Home") next = MIN_SPLIT_RATIO;
    if (event.key === "End") next = MAX_SPLIT_RATIO;
    if (event.key === (vertical ? "ArrowLeft" : "ArrowUp")) next = ratio - 0.05;
    if (event.key === (vertical ? "ArrowRight" : "ArrowDown")) next = ratio + 0.05;
    if (next === null) return;
    event.preventDefault();
    update(next);
    onCommit(splitId, clampRatio(next));
  };

  const startDragging = (event: React.PointerEvent<HTMLDivElement>) => {
    const container = event.currentTarget.parentElement;
    if (!container) return;
    event.preventDefault();
    const bounds = container.getBoundingClientRect();
    const size = vertical ? bounds.width : bounds.height;
    if (size <= 0) return;
    const startPosition = vertical ? event.clientX : event.clientY;
    const startRatio = ratio;
    const move = (pointer: PointerEvent) => {
      const position = vertical ? pointer.clientX : pointer.clientY;
      update(startRatio + (position - startPosition) / size);
    };
    const finish = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      onCommit(splitId, latestRatio.current);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish, { once: true });
  };

  return <div className={`split-divider ${direction}`} style={style} role="separator" tabIndex={0} aria-orientation={vertical ? "vertical" : "horizontal"} aria-valuemin={15} aria-valuemax={85} aria-valuenow={Math.round(ratio * 100)} aria-label={`Ajustar divisão ${vertical ? "vertical" : "horizontal"}`} title="Arraste para ajustar. Use setas, Home ou End pelo teclado." onKeyDown={adjustFromKeyboard} onPointerDown={startDragging} />;
}

function ContextMenu({ menu, onClose }: { menu: NonNullable<ContextMenuState>; onClose: () => void }) {
  return <div className="context-menu" role="menu" style={{ left: menu.x, top: menu.y }} onClick={(event) => event.stopPropagation()}>{menu.entries.map((entry) => <button role="menuitem" key={entry.label} title={entry.description} onClick={() => { onClose(); entry.action(); }}><strong>{entry.label}</strong><small>{entry.description}</small></button>)}</div>;
}

function TextEditDialog({ workspaceKind, onWorkspaceKind, dialog, value, error, onValue, onClose, onSubmit }: { workspaceKind: WorkspaceKind; onWorkspaceKind: (kind: WorkspaceKind) => void; dialog: TextDialogState; value: string; error: string | null; onValue: (value: string) => void; onClose: () => void; onSubmit: (event: React.FormEvent) => void }) {
  const dialogRef = useRef<HTMLFormElement>(null);
  const isLabel = dialog.kind === "terminal-label";
  const copy = isLabel
    ? {
        title: "Etiqueta do terminal",
        field: "Etiqueta",
        submit: "Salvar etiqueta",
        hint: "Use uma marca curta para identificar o painel. Deixe em branco para remover a etiqueta.",
      }
    : dialog.kind === "create-workspace"
      ? {
          title: "Novo workspace",
          field: "Nome do workspace",
          submit: "Criar workspace",
          hint: "O workspace organiza os painéis desta Área e não altera arquivos da pasta.",
        }
      : {
          title: dialog.kind === "rename-area" ? "Renomear área de trabalho" : dialog.kind === "rename-workspace" ? "Renomear workspace" : "Renomear terminal",
          field: "Nome",
          submit: "Salvar nome",
          hint: "Esta alteração atualiza apenas o nome exibido no KanashaTerminal.",
        };

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialogElement = dialogRef.current;
    const focusable = () => Array.from(
      dialogElement?.querySelectorAll<HTMLElement>(
        "button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1'])",
      ) ?? [],
    );
    dialogElement?.querySelector<HTMLInputElement>("input")?.focus();
    const trapFocus = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const elements = focusable();
      if (elements.length === 0) return;
      const first = elements[0];
      const last = elements[elements.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", trapFocus);
    return () => {
      document.removeEventListener("keydown", trapFocus);
      previousFocus?.focus();
    };
  }, []);

  return <div className="modal-backdrop"><form ref={dialogRef} className="modal" role="dialog" aria-modal="true" aria-labelledby="text-edit-dialog-title" onSubmit={onSubmit}><header><h2 id="text-edit-dialog-title">{copy.title}</h2><button type="button" title="Fechar" aria-label="Fechar" onClick={onClose}>×</button></header><label htmlFor="text-edit-dialog-value">{copy.field}</label><input id="text-edit-dialog-value" required={!isLabel} maxLength={isLabel ? 32 : 120} value={value} onChange={(event) => onValue(event.target.value)} aria-invalid={Boolean(error)} aria-describedby={error ? "text-edit-dialog-hint text-edit-dialog-error" : "text-edit-dialog-hint"} autoFocus />{dialog.kind === "create-workspace" ? <WorkspaceKindField value={workspaceKind} onChange={onWorkspaceKind} /> : null}<p id="text-edit-dialog-hint">{copy.hint}</p>{error ? <p className="form-error" id="text-edit-dialog-error" role="alert">{error}</p> : null}<footer><button type="button" title="Cancelar edição" onClick={onClose}>Cancelar</button><button type="submit" title={copy.submit} disabled={!isLabel && !value.trim()}>{copy.submit}</button></footer></form></div>;
}

function WorkspaceKindField({ value, onChange, label = "Tipo de workspace" }: { value: WorkspaceKind; onChange: (kind: WorkspaceKind) => void; label?: string }) {
  return <><label htmlFor="workspace-kind">{label}</label><select id="workspace-kind" value={value} onChange={event => onChange(event.target.value as WorkspaceKind)}><option value="normal">Organização normal</option><option value="team">Equipe</option></select><p>{value === "normal" ? "Organize terminais livremente, sem gerente ou contratação." : "Comece com um gerente e contrate membros na tela Equipe."}</p></>;
}

function AreaDialog({ workspaceKind, onWorkspaceKind, name, path, onName, onPath, onChoose, onClose, onSubmit }: { workspaceKind: WorkspaceKind; onWorkspaceKind: (kind: WorkspaceKind) => void; name: string; path: string; onName: (value: string) => void; onPath: (value: string) => void; onChoose: () => void; onClose: () => void; onSubmit: (event: React.FormEvent) => void }) {
  return <div className="modal-backdrop"><form className="modal" role="dialog" aria-modal="true" aria-labelledby="area-dialog-title" onSubmit={onSubmit}><header><h2 id="area-dialog-title">Nova área de trabalho</h2><button type="button" title="Fechar" aria-label="Fechar" onClick={onClose}>×</button></header><label htmlFor="area-name">Nome</label><input id="area-name" required value={name} onChange={(event) => onName(event.target.value)} placeholder="Ex.: Plataforma Kanasha" autoFocus /><label htmlFor="area-path">Pasta-raiz local</label><div className="path-field"><input id="area-path" required value={path} onChange={(event) => onPath(event.target.value)} placeholder="/caminho/para/projetos" /><button type="button" title="Selecionar pasta local" onClick={onChoose}>Escolher…</button></div><WorkspaceKindField value={workspaceKind} onChange={onWorkspaceKind} label="Tipo do primeiro workspace" /><footer><button type="button" title="Cancelar criação da área" onClick={onClose}>Cancelar</button><button type="submit" title="Criar área de trabalho">Criar área</button></footer></form></div>;
}

function ProfileDialog({ editing, name, executable, argumentsText, onName, onExecutable, onArguments, onClose, onSubmit }: { editing: boolean; name: string; executable: string; argumentsText: string; onName: (value: string) => void; onExecutable: (value: string) => void; onArguments: (value: string) => void; onClose: () => void; onSubmit: (event: React.FormEvent) => void }) {
  return <div className="modal-backdrop"><form className="modal" role="dialog" aria-modal="true" aria-labelledby="profile-dialog-title" onSubmit={onSubmit}><header><h2 id="profile-dialog-title">{editing ? "Configurar perfil nesta sessão" : "Novo perfil local"}</h2><button type="button" title="Fechar" aria-label="Fechar" onClick={onClose}>×</button></header>{editing ? <p><strong>Perfil:</strong> {name}</p> : <><label htmlFor="profile-name">Nome</label><input id="profile-name" required value={name} onChange={(event) => onName(event.target.value)} placeholder="Ex.: Antigravity" autoFocus /></>}<label htmlFor="profile-executable">Executável local</label><input id="profile-executable" required value={executable} onChange={(event) => onExecutable(event.target.value)} placeholder="antigravity" autoFocus={editing} /><label htmlFor="profile-arguments">Argumentos, um por linha</label><textarea id="profile-arguments" value={argumentsText} onChange={(event) => onArguments(event.target.value)} placeholder="--modo\nassistido" rows={3} /><p>O executável e os argumentos ficam apenas nesta sessão e não são gravados pela aplicação.</p><footer><button type="button" title="Cancelar configuração do perfil" onClick={onClose}>Cancelar</button><button type="submit" title={editing ? "Configurar perfil nesta sessão" : "Criar perfil local"}>{editing ? "Configurar" : "Criar perfil"}</button></footer></form></div>;
}

export default App;
