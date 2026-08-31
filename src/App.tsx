import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useMemo, useRef, useState } from "react";
import { terminalApi } from "./api";
import { TerminalPanel } from "./components/TerminalPanel";
import type {
  AppSnapshot,
  Area,
  LayoutNode,
  ProfileView,
  SplitDirection,
  TerminalPanelModel,
  Workspace,
} from "./types";
import "./App.css";

type MenuEntry = { label: string; description: string; action: () => void };
type ContextMenuState = { x: number; y: number; entries: MenuEntry[] } | null;
type TextDialogState =
  | { kind: "create-workspace"; areaId: string }
  | { kind: "rename-area"; areaId: string }
  | { kind: "rename-workspace"; workspaceId: string }
  | { kind: "rename-terminal"; terminalId: string }
  | { kind: "terminal-label"; terminalId: string };

const MIN_SPLIT_RATIO = 0.15;
const MAX_SPLIT_RATIO = 0.85;

function clampRatio(value: number) {
  return Math.min(MAX_SPLIT_RATIO, Math.max(MIN_SPLIT_RATIO, value));
}

function panelIdsInVisualOrder(node: LayoutNode): string[] {
  return node.kind === "panel"
    ? [node.panelId]
    : [...panelIdsInVisualOrder(node.first), ...panelIdsInVisualOrder(node.second)];
}

function containsPanel(node: LayoutNode, panelId: string): boolean {
  return node.kind === "panel"
    ? node.panelId === panelId
    : containsPanel(node.first, panelId) || containsPanel(node.second, panelId);
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
  const [profileDialogOpen, setProfileDialogOpen] = useState(false);
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
  const closingAfterConfirmation = useRef(false);

  const selectedArea = useMemo(
    () => snapshot?.areas.find((area) => area.id === selectedAreaId) ?? null,
    [snapshot, selectedAreaId],
  );
  const selectedWorkspace = useMemo(
    () => selectedArea?.workspaces.find((workspace) => workspace.id === selectedWorkspaceId) ?? null,
    [selectedArea, selectedWorkspaceId],
  );
  const activeIds = useMemo(
    () => new Set(snapshot?.activeTerminalIds ?? []),
    [snapshot?.activeTerminalIds],
  );
  const visiblePanelIds = useMemo(
    () => (selectedWorkspace?.layout ? panelIdsInVisualOrder(selectedWorkspace.layout) : []),
    [selectedWorkspace?.layout],
  );

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

  function showMenu(event: React.MouseEvent, entries: MenuEntry[]) {
    event.preventDefault();
    setContextMenu({ x: event.clientX, y: event.clientY, entries });
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
    await run(terminalApi.createArea({ name: areaName, rootPath: areaPath }));
    setAreaDialogOpen(false);
    setAreaName("");
    setAreaPath("");
  }

  function openCreateWorkspaceDialog() {
    if (!selectedArea) return;
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
          next = await terminalApi.createWorkspace({ areaId: textDialog.areaId, name: value });
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

  function profileMenu(profile: ProfileView): MenuEntry[] {
    if (profile.builtIn) return [];
    return [
      {
        label: "Configurar nesta sessão",
        description: "Informa executável e argumentos apenas para a sessão atual.",
        action: () => openProfileConfiguration(profile),
      },
      {
        label: "Remover perfil",
        description: "Remove o metadado local; não remove arquivos nem comandos.",
        action: () => {
          if (window.confirm(`Remover o perfil ${profile.name}?`)) {
            void run(terminalApi.removeCustomProfile(profile.id));
          }
        },
      },
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
        <div className="profile-list">
          <div className="sidebar-heading"><span>Perfis</span><button title="Criar perfil local" aria-label="Criar perfil local" onClick={openNewProfileDialog}>+</button></div>
          {snapshot.profiles.map((profile) => (
            <div className="profile-row" key={profile.id}>
              <button className={`profile-item ${profile.id === selectedProfileId ? "selected" : ""}`} title={profile.available ? `Usar perfil ${profile.name}` : `${profile.name} indisponível ou não configurado`} onClick={() => setSelectedProfileId(profile.id)} onContextMenu={profile.builtIn ? undefined : (event) => showMenu(event, profileMenu(profile))}>
                <span className={`availability ${profile.available ? "available" : "unavailable"}`} aria-hidden="true" /><span>{profile.name}</span>
              </button>
              {!profile.builtIn ? <button className="context-trigger" title={`Ações do perfil ${profile.name}`} aria-label={`Ações do perfil ${profile.name}`} aria-haspopup="menu" onClick={(event) => showMenuFromButton(event, profileMenu(profile))}>⋯</button> : null}
            </div>
          ))}
        </div>
      </aside>

      <section className="workspace-shell">
        {selectedArea ? <>
          <header className="workspace-tabs" aria-label="Workspaces">
            <span className="area-path" title={selectedArea.rootPath}>{selectedArea.name}</span>
            <div className="tabs" role="tablist" aria-label={`Workspaces de ${selectedArea.name}`}>
              {selectedArea.workspaces.map((workspace) => (
                <div className="tab-group" key={workspace.id}>
                  <button role="tab" aria-selected={workspace.id === selectedWorkspace?.id} className={workspace.id === selectedWorkspace?.id ? "tab selected" : "tab"} title={`Selecionar workspace ${workspace.name}`} onClick={() => setSelectedWorkspaceId(workspace.id)} onContextMenu={(event) => showMenu(event, workspaceMenu(workspace))}>{workspace.name}</button>
                  <button className="context-trigger tab-context-trigger" title={`Ações do workspace ${workspace.name}`} aria-label={`Ações do workspace ${workspace.name}`} aria-haspopup="menu" onClick={(event) => showMenuFromButton(event, workspaceMenu(workspace))}>⋯</button>
                </div>
              ))}
              <button className="tab add-tab" title="Criar novo workspace" aria-label="Criar novo workspace" onClick={openCreateWorkspaceDialog}>+</button>
            </div>
          </header>
          {selectedWorkspace ? <>
            <div className="workspace-toolbar">
              <label htmlFor="profile-select">Perfil para novos painéis</label>
              <select id="profile-select" value={selectedProfileId} onChange={(event) => setSelectedProfileId(event.target.value)}>{snapshot.profiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}{profile.available ? "" : " — indisponível"}</option>)}</select>
              <button title="Adicionar novo terminal" onClick={() => void addTerminal()}>+ Terminal</button>
              <span className="privacy-note">Nada digitado ou exibido no terminal é salvo.</span>
            </div>
            <section className={`terminal-stage ${maximizedPanelId ? "is-maximized" : ""}`} aria-label={`Terminais de ${selectedWorkspace.name}`}>
              {snapshot.areas.flatMap((area) => area.workspaces.map((workspace) => {
                const isSelected = workspace.id === selectedWorkspace.id;
                return <div className={`workspace-terminal-layout ${isSelected ? "" : "is-hidden"}`} key={workspace.id} aria-hidden={!isSelected}>
                  {workspace.layout ? <LayoutTree node={workspace.layout} panels={workspace.panels} activeIds={activeIds} focusedPanelId={focusedPanelId} maximizedPanelId={maximizedPanelId} pendingSplitRatios={pendingSplitRatios} onStart={(id) => void run(terminalApi.startTerminal(id))} onStop={(id) => { if (window.confirm("Encerrar este processo?")) void run(terminalApi.stopTerminal(id)); }} onClose={(id) => void stopAndRemoveTerminal(id)} onSplit={(id, direction) => void addTerminal(id, direction)} onFocus={setFocusedPanelId} onToggleMaximize={(id) => { setFocusedPanelId(id); setMaximizedPanelId((current) => current === id ? null : id); }} onRatioChange={(splitId, ratio) => setPendingSplitRatios((current) => ({ ...current, [splitId]: ratio }))} onRatioCommit={(splitId, ratio) => void persistSplitRatio(splitId, ratio)} onContextMenu={(event, id) => { const panel = workspace.panels.find((item) => item.id === id); if (panel) showMenu(event, terminalMenu(panel)); }} /> : <div className="empty-workspace"><h1>{workspace.name}</h1><p>Abra o primeiro terminal para começar. Ele iniciará na pasta-raiz da Área.</p><button title="Abrir primeiro terminal" onClick={() => void addTerminal()}>Abrir terminal</button></div>}
                </div>;
              }))}
            </section>
          </> : null}
        </> : <section className="empty-workspace onboarding"><p className="eyebrow">Primeiro uso</p><h1>Organize seus agentes por área e workspace.</h1><p>Uma Área aponta para uma pasta-raiz local. Dentro dela você cria workspaces e abre terminais em paralelo.</p><button title="Criar primeira área" onClick={() => setAreaDialogOpen(true)}>Criar primeira área</button></section>}
      </section>

      {error ? <div className="error-toast" role="alert">{error}<button title="Fechar mensagem" onClick={() => setError(null)}>×</button></div> : null}
      {contextMenu ? <ContextMenu menu={contextMenu} onClose={() => setContextMenu(null)} /> : null}
      {areaDialogOpen ? <AreaDialog name={areaName} path={areaPath} onName={setAreaName} onPath={setAreaPath} onChoose={() => void chooseFolder(setAreaPath)} onClose={() => setAreaDialogOpen(false)} onSubmit={createArea} /> : null}
      {profileDialogOpen ? <ProfileDialog editing={configuredProfileId !== null} name={profileName} executable={profileExecutable} argumentsText={profileArguments} onName={setProfileName} onExecutable={setProfileExecutable} onArguments={setProfileArguments} onClose={closeProfileDialog} onSubmit={createProfile} /> : null}
      {textDialog ? <TextEditDialog dialog={textDialog} value={textDialogValue} error={textDialogError} onValue={setTextDialogValue} onClose={closeTextDialog} onSubmit={saveTextDialog} /> : null}
    </main>
  );
}

type LayoutTreeProps = {
  node: LayoutNode;
  panels: TerminalPanelModel[];
  activeIds: Set<string>;
  focusedPanelId: string | null;
  maximizedPanelId: string | null;
  pendingSplitRatios: Record<string, number>;
  onStart: (id: string) => void;
  onStop: (id: string) => void;
  onClose: (id: string) => void;
  onSplit: (id: string, direction: SplitDirection) => void;
  onFocus: (id: string) => void;
  onToggleMaximize: (id: string) => void;
  onRatioChange: (splitId: string, ratio: number) => void;
  onRatioCommit: (splitId: string, ratio: number) => void;
  onContextMenu: (event: React.MouseEvent, id: string) => void;
};

function LayoutTree({ node, panels, activeIds, focusedPanelId, maximizedPanelId, pendingSplitRatios, onStart, onStop, onClose, onSplit, onFocus, onToggleMaximize, onRatioChange, onRatioCommit, onContextMenu }: LayoutTreeProps) {
  if (node.kind === "panel") {
    const panel = panels.find((item) => item.id === node.panelId);
    return panel ? <TerminalPanel panel={panel} active={activeIds.has(panel.id)} focused={focusedPanelId === panel.id} maximized={maximizedPanelId === panel.id} onStart={onStart} onStop={onStop} onClose={onClose} onSplit={onSplit} onFocus={onFocus} onToggleMaximize={onToggleMaximize} onContextMenu={onContextMenu} /> : null;
  }
  const ratio = clampRatio(pendingSplitRatios[node.id] ?? node.ratio);
  const firstMaximized = Boolean(maximizedPanelId && containsPanel(node.first, maximizedPanelId));
  const secondMaximized = Boolean(maximizedPanelId && containsPanel(node.second, maximizedPanelId));
  const showFirst = !maximizedPanelId || firstMaximized;
  const showSecond = !maximizedPanelId || secondMaximized;
  const bothVisible = showFirst && showSecond;
  const gridStyle = node.direction === "vertical"
    ? { gridTemplateColumns: bothVisible ? `minmax(0, ${ratio}fr) 9px minmax(0, ${1 - ratio}fr)` : "minmax(0, 1fr)" }
    : { gridTemplateRows: bothVisible ? `minmax(0, ${ratio}fr) 9px minmax(0, ${1 - ratio}fr)` : "minmax(0, 1fr)" };
  const childProps = { panels, activeIds, focusedPanelId, maximizedPanelId, pendingSplitRatios, onStart, onStop, onClose, onSplit, onFocus, onToggleMaximize, onRatioChange, onRatioCommit, onContextMenu };
  return <div className={`terminal-split ${node.direction}`} style={gridStyle}>
    <div className={`layout-branch ${showFirst ? "" : "is-hidden"}`}><LayoutTree node={node.first} {...childProps} /></div>
    {bothVisible ? <SplitDivider splitId={node.id} direction={node.direction} ratio={ratio} onChange={onRatioChange} onCommit={onRatioCommit} /> : null}
    <div className={`layout-branch ${showSecond ? "" : "is-hidden"}`}><LayoutTree node={node.second} {...childProps} /></div>
  </div>;
}

function SplitDivider({ splitId, direction, ratio, onChange, onCommit }: { splitId: string; direction: SplitDirection; ratio: number; onChange: (splitId: string, ratio: number) => void; onCommit: (splitId: string, ratio: number) => void }) {
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

  return <div className={`split-divider ${direction}`} role="separator" tabIndex={0} aria-orientation={vertical ? "vertical" : "horizontal"} aria-valuemin={15} aria-valuemax={85} aria-valuenow={Math.round(ratio * 100)} aria-label={`Ajustar divisão ${vertical ? "vertical" : "horizontal"}`} title="Arraste para ajustar. Use setas, Home ou End pelo teclado." onKeyDown={adjustFromKeyboard} onPointerDown={startDragging} />;
}

function ContextMenu({ menu, onClose }: { menu: NonNullable<ContextMenuState>; onClose: () => void }) {
  return <div className="context-menu" role="menu" style={{ left: menu.x, top: menu.y }} onClick={(event) => event.stopPropagation()}>{menu.entries.map((entry) => <button role="menuitem" key={entry.label} title={entry.description} onClick={() => { onClose(); entry.action(); }}><strong>{entry.label}</strong><small>{entry.description}</small></button>)}</div>;
}

function TextEditDialog({ dialog, value, error, onValue, onClose, onSubmit }: { dialog: TextDialogState; value: string; error: string | null; onValue: (value: string) => void; onClose: () => void; onSubmit: (event: React.FormEvent) => void }) {
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

  return <div className="modal-backdrop"><form ref={dialogRef} className="modal" role="dialog" aria-modal="true" aria-labelledby="text-edit-dialog-title" onSubmit={onSubmit}><header><h2 id="text-edit-dialog-title">{copy.title}</h2><button type="button" title="Fechar" aria-label="Fechar" onClick={onClose}>×</button></header><label htmlFor="text-edit-dialog-value">{copy.field}</label><input id="text-edit-dialog-value" required={!isLabel} maxLength={isLabel ? 32 : 120} value={value} onChange={(event) => onValue(event.target.value)} aria-invalid={Boolean(error)} aria-describedby={error ? "text-edit-dialog-hint text-edit-dialog-error" : "text-edit-dialog-hint"} autoFocus /><p id="text-edit-dialog-hint">{copy.hint}</p>{error ? <p className="form-error" id="text-edit-dialog-error" role="alert">{error}</p> : null}<footer><button type="button" title="Cancelar edição" onClick={onClose}>Cancelar</button><button type="submit" title={copy.submit} disabled={!isLabel && !value.trim()}>{copy.submit}</button></footer></form></div>;
}

function AreaDialog({ name, path, onName, onPath, onChoose, onClose, onSubmit }: { name: string; path: string; onName: (value: string) => void; onPath: (value: string) => void; onChoose: () => void; onClose: () => void; onSubmit: (event: React.FormEvent) => void }) {
  return <div className="modal-backdrop"><form className="modal" role="dialog" aria-modal="true" aria-labelledby="area-dialog-title" onSubmit={onSubmit}><header><h2 id="area-dialog-title">Nova área de trabalho</h2><button type="button" title="Fechar" aria-label="Fechar" onClick={onClose}>×</button></header><label htmlFor="area-name">Nome</label><input id="area-name" required value={name} onChange={(event) => onName(event.target.value)} placeholder="Ex.: Plataforma Kanasha" autoFocus /><label htmlFor="area-path">Pasta-raiz local</label><div className="path-field"><input id="area-path" required value={path} onChange={(event) => onPath(event.target.value)} placeholder="/caminho/para/projetos" /><button type="button" title="Selecionar pasta local" onClick={onChoose}>Escolher…</button></div><p>Essa ação nunca altera os arquivos da pasta selecionada.</p><footer><button type="button" title="Cancelar criação da área" onClick={onClose}>Cancelar</button><button type="submit" title="Criar área de trabalho">Criar área</button></footer></form></div>;
}

function ProfileDialog({ editing, name, executable, argumentsText, onName, onExecutable, onArguments, onClose, onSubmit }: { editing: boolean; name: string; executable: string; argumentsText: string; onName: (value: string) => void; onExecutable: (value: string) => void; onArguments: (value: string) => void; onClose: () => void; onSubmit: (event: React.FormEvent) => void }) {
  return <div className="modal-backdrop"><form className="modal" role="dialog" aria-modal="true" aria-labelledby="profile-dialog-title" onSubmit={onSubmit}><header><h2 id="profile-dialog-title">{editing ? "Configurar perfil nesta sessão" : "Novo perfil local"}</h2><button type="button" title="Fechar" aria-label="Fechar" onClick={onClose}>×</button></header>{editing ? <p><strong>Perfil:</strong> {name}</p> : <><label htmlFor="profile-name">Nome</label><input id="profile-name" required value={name} onChange={(event) => onName(event.target.value)} placeholder="Ex.: Antigravity" autoFocus /></>}<label htmlFor="profile-executable">Executável local</label><input id="profile-executable" required value={executable} onChange={(event) => onExecutable(event.target.value)} placeholder="antigravity" autoFocus={editing} /><label htmlFor="profile-arguments">Argumentos, um por linha</label><textarea id="profile-arguments" value={argumentsText} onChange={(event) => onArguments(event.target.value)} placeholder="--modo\nassistido" rows={3} /><p>O executável e os argumentos ficam apenas nesta sessão e não são gravados pela aplicação.</p><footer><button type="button" title="Cancelar configuração do perfil" onClick={onClose}>Cancelar</button><button type="submit" title={editing ? "Configurar perfil nesta sessão" : "Criar perfil local"}>{editing ? "Configurar" : "Criar perfil"}</button></footer></form></div>;
}

export default App;
