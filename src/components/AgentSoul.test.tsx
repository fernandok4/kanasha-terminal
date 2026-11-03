import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AgentSoul } from "./AgentSoul";
import type { AppSnapshot, TerminalPanelModel } from "../types";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const person: TerminalPanelModel = { id: "manager", title: "Beatriz", profileId: "codex", member: {
  manager: true, specialty: "Coordenação", instructions: "Coordenar", paused: false, threadId: null, jobs: [], training: "review",
  memory: { content: "Aprendizado de uma demanda recente.", revision: 3, updatedAt: 1, updatedBy: "manager" },
  soul: { content: "## Missão\nCuidar das entregas da equipe.", revision: 2, updatedAt: 1, updatedBy: "manager",
    approved: { content: "## Missão\nIdentidade anterior aprovada.", revision: 1, approvedAt: 1, approvedBy: "user" } },
} };
function snapshot(value: TerminalPanelModel): AppSnapshot {
  return { areas: [{ id: "area", name: "Empresa", rootPath: "/tmp", workspaces: [{ id: "room", name: "Equipe", layout: null, panels: [value] }] }], profiles: [], activeTerminalIds: [] };
}
beforeEach(() => { invoke.mockReset(); });
afterEach(cleanup);

it("separa a identidade aprovada, proposta e memória e aprova as duas revisões lidas", async () => {
  invoke.mockResolvedValue(snapshot(person));
  render(<AgentSoul person={person} onSnapshot={vi.fn()} onClose={vi.fn()} />);
  expect(screen.getByText("Identidade anterior aprovada.")).toBeInTheDocument();
  expect(screen.getByLabelText("Proposta de alma em Markdown")).toHaveValue(person.member!.soul!.content);
  expect(screen.getByText("Aprendizado de uma demanda recente.")).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Proposta de alma em Markdown"), { target: { value: "Identidade corrigida" } });
  expect(screen.getByRole("button", { name: "Revisado: concluir treinamento" })).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Proposta de alma em Markdown"), { target: { value: person.member!.soul!.content } });
  fireEvent.click(screen.getByRole("button", { name: "Revisado: concluir treinamento" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("complete_training", { terminalId: "manager", expectedRevision: 3, expectedSoulRevision: 2 }));
});

it("permite formação inicial com alma e sem memória posterior", async () => {
  const initial = structuredClone(person);
  initial.member!.memory = { content: "", revision: 0, updatedAt: null, updatedBy: null };
  initial.member!.soul!.approved = null;
  invoke.mockResolvedValue(snapshot(initial));
  render(<AgentSoul person={initial} onSnapshot={vi.fn()} onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "Revisado: concluir treinamento" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("complete_training", { terminalId: "manager", expectedRevision: 0, expectedSoulRevision: 2 }));
});

it("salva proposta sem aprovar e preserva o rascunho após conflito", async () => {
  invoke.mockRejectedValue("A alma mudou. Consulte a versão atual.");
  render(<AgentSoul person={person} onSnapshot={vi.fn()} onClose={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Proposta de alma em Markdown"), { target: { value: "Correção do usuário" } });
  fireEvent.click(screen.getByRole("button", { name: "Salvar proposta de alma" }));
  await screen.findByRole("alert");
  expect(invoke).toHaveBeenCalledWith("set_agent_soul", { terminalId: "manager", content: "Correção do usuário", expectedRevision: 2 });
  expect(screen.getByLabelText("Proposta de alma em Markdown")).toHaveValue("Correção do usuário");
  expect(screen.getByText("Identidade anterior aprovada.")).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledTimes(1);
});

it("preserva edição concorrente e impede aprovação de uma versão desatualizada", () => {
  const { rerender } = render(<AgentSoul person={person} onSnapshot={vi.fn()} onClose={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Proposta de alma em Markdown"), { target: { value: "Rascunho local" } });
  const changed = structuredClone(person);
  changed.member!.soul!.revision = 3;
  changed.member!.soul!.content = "Outro ensinamento";
  rerender(<AgentSoul person={changed} onSnapshot={vi.fn()} onClose={vi.fn()} />);
  expect(screen.getByRole("alert")).toHaveTextContent("rascunho foi preservado");
  expect(screen.getByLabelText("Proposta de alma em Markdown")).toHaveValue("Rascunho local");
  expect(screen.getByRole("button", { name: "Revisado: concluir treinamento" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Salvar proposta de alma" })).toBeDisabled();
});

it("aprova uma redefinição explícita sem confundir com conclusão de treinamento", async () => {
  const trained = structuredClone(person);
  trained.member!.training = "ready";
  const approved = structuredClone(trained);
  approved.member!.soul!.approved = { content: trained.member!.soul!.content, revision: 2, approvedAt: 2, approvedBy: "user" };
  invoke.mockResolvedValue(snapshot(approved));
  render(<AgentSoul person={trained} onSnapshot={vi.fn()} onClose={vi.fn()} />);
  expect(screen.queryByRole("button", { name: "Revisado: concluir treinamento" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Aprovar alma" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("approve_agent_soul", { terminalId: "manager", expectedRevision: 2 }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Aprovar alma" })).toBeDisabled());
});

it("não converte perfil ou memória antigos em alma automaticamente", () => {
  const legacy = structuredClone(person);
  delete legacy.member!.soul;
  render(<AgentSoul person={legacy} onSnapshot={vi.fn()} onClose={vi.fn()} />);
  expect(screen.getByLabelText("Proposta de alma em Markdown")).toHaveValue("");
  expect(screen.getByRole("button", { name: "Revisado: concluir treinamento" })).toBeDisabled();
  expect(invoke).not.toHaveBeenCalled();
});
