import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { OfficeView } from "./OfficeView";
import type { AppSnapshot, Area, TerminalPanelModel } from "../types";

afterEach(cleanup);

const person: TerminalPanelModel = { id: "ana", title: "Ana", profileId: "codex", member: { specialty: "Backend", instructions: "", manager: true, paused: false, jobs: [], threadId: "saved-thread" } };
const area: Area = { id: "office", name: "Engenharia", rootPath: "/tmp", workspaces: [{ id: "team", kind: "team", name: "Plataforma", panels: [person], layout: { kind: "panel", panelId: "ana" } }] };
const snapshot: AppSnapshot = { areas: [area], profiles: [], activeTerminalIds: [] };

describe("OfficeView", () => {
  it("exclui organizações normais do mapa e da contagem", () => {
    const mixed: Area = { ...area, workspaces: [...area.workspaces, { id: "normal", kind: "normal", name: "Terminais livres", panels: [], layout: null }] };
    render(<OfficeView area={mixed} snapshot={snapshot} onOpen={vi.fn()} onTeam={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getByText("1 baia · 1 pessoa")).toBeVisible();
    expect(screen.queryByText("Terminais livres")).not.toBeInTheDocument();
  });

  it("atualiza a mesa com o snapshot e prioriza perguntas e pausas sobre status antigos", () => {
    const props = { area, onOpen: vi.fn(), onTeam: vi.fn(), onClose: vi.fn() };
    const { rerender } = render(<OfficeView {...props} snapshot={{ ...snapshot, agentStates: { ana: { status: "working" } } }} />);
    const desk = screen.getByRole("button", { name: "Abrir terminal de Ana — Plataforma" });
    expect(within(desk).getByText("Desconectado")).toBeInTheDocument();
    const connected = { ...snapshot, memberStates: { ana: { connected: true, turnId: "turn", activity: "", updatedAt: 0 } } };
    rerender(<OfficeView {...props} snapshot={connected} />);
    expect(within(desk).getByText("Trabalhando")).toBeInTheDocument();
    rerender(<OfficeView {...props} snapshot={{ ...connected, memberStates: { ana: { ...connected.memberStates.ana, request: { id: 1, method: "question", params: {} } } } }} />);
    expect(within(desk).getByText("Precisa de resposta")).toBeInTheDocument();
    const pausedArea = { ...area, workspaces: [{ ...area.workspaces[0], panels: [{ ...person, member: { ...person.member!, paused: true } }] }] };
    rerender(<OfficeView {...props} area={pausedArea} snapshot={connected} />);
    expect(within(desk).getByText("Pausado")).toBeInTheDocument();
  });

  it("mostra baias vazias e encaminha a gestão ao time certo", () => {
    const onTeam = vi.fn();
    const emptyArea = { ...area, workspaces: [{ ...area.workspaces[0], panels: [] }] };
    render(<OfficeView area={emptyArea} snapshot={snapshot} onOpen={vi.fn()} onTeam={onTeam} onClose={vi.fn()} />);
    expect(screen.getByText(/Mesas livres/)).toBeVisible();
    expect(screen.queryByRole("button", { name: /Abrir terminal de/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Ver equipe de Plataforma" }));
    expect(onTeam).toHaveBeenCalledWith("team");
  });
});
