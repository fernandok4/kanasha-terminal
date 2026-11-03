import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AgentMemory } from "./AgentMemory";
import type { TerminalPanelModel } from "../types";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const person: TerminalPanelModel = { id: "manager", title: "Marina", profileId: "codex", member: { manager: true, specialty: "Coordenação", instructions: "Coordenar", paused: false, threadId: null, jobs: [], training: "review", soul: { content: "# Missão\nCoordenar a equipe.", revision: 2, updatedAt: 1, updatedBy: "manager", approved: null }, memory: { content: "# Empresa\nValidar jornadas.", revision: 3, updatedAt: 1, updatedBy: "manager" } } };
beforeEach(() => { invoke.mockReset(); });
afterEach(cleanup);
it("aprova somente a versão revisada e exige salvar correções antes", async () => {
  invoke.mockResolvedValue({ areas: [] });
  render(<AgentMemory person={person} onSnapshot={vi.fn()} onClose={vi.fn()} />);
  expect(screen.getByLabelText("Aprendizado em Markdown")).toHaveValue(person.member?.memory?.content);
  fireEvent.change(screen.getByLabelText("Aprendizado em Markdown"), { target: { value: "# Correção" } });
  expect(screen.getByRole("button", { name: "Revisado: concluir treinamento" })).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Aprendizado em Markdown"), { target: { value: person.member?.memory?.content } });
  fireEvent.click(screen.getByRole("button", { name: "Revisado: concluir treinamento" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("complete_training", { terminalId: "manager", expectedRevision: 3, expectedSoulRevision: 2 }));
});
it("preserva correções se a memória foi atualizada durante a revisão", async () => {
  invoke.mockRejectedValue("A memória mudou. Consulte a versão atual.");
  render(<AgentMemory person={person} onSnapshot={vi.fn()} onClose={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("Aprendizado em Markdown"), { target: { value: "# Correção do usuário" } });
  fireEvent.click(screen.getByRole("button", { name: "Salvar memória" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("A memória mudou");
  expect(screen.getByLabelText("Aprendizado em Markdown")).toHaveValue("# Correção do usuário");
});
