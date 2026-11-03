import { useState } from "react";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AgentExecutionFields } from "./AgentExecutionFields";
import type { AgentExecution, CodexModel } from "../types";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
afterEach(cleanup);
beforeEach(() => invoke.mockReset());
const option = (model: string, efforts: string[]): CodexModel => ({ model, displayName: model, defaultReasoningEffort: "medium", supportedReasoningEfforts: efforts.map(reasoningEffort => ({ reasoningEffort, description: "" })) });
const initial: AgentExecution = { level: "manager", model: "gpt-6-sol", reasoningEffort: "ultra" };
function Editor() {
  const [value, setValue] = useState(initial);
  return <><AgentExecutionFields workspaceId="room" value={value} manager onChange={setValue} /><output>{JSON.stringify(value)}</output></>;
}

it("mostra modelos novos e limita raciocínio ao catálogo, preservando a escolha até ação do usuário", async () => {
  invoke.mockResolvedValue([option("gpt-6-sol", ["medium", "ultra"]), option("gpt-6.1-sol", ["medium", "ultra"]), option("gpt-5.6-luna", ["low", "medium"])]);
  render(<Editor />);
  await waitFor(() => expect(screen.getByLabelText("Modelo")).toBeEnabled());
  expect(invoke).toHaveBeenCalledWith("list_codex_models", { workspaceId: "room" });
  expect(screen.getByLabelText("Modelo")).toHaveValue("gpt-6-sol");
  fireEvent.change(screen.getByLabelText("Modelo"), { target: { value: "gpt-6.1-sol" } });
  expect(screen.getByLabelText("Raciocínio")).toHaveValue("ultra");
  fireEvent.change(screen.getByLabelText("Modelo"), { target: { value: "gpt-5.6-luna" } });
  expect(screen.getByLabelText("Raciocínio")).toHaveValue("medium");
  expect(within(screen.getByLabelText("Raciocínio")).queryByRole("option", { name: "ultra" })).not.toBeInTheDocument();
  expect(screen.getByLabelText("Nível")).toHaveValue("manager");
});

it("preserva o modelo salvo ausente do catálogo, sem substituição automática", async () => {
  invoke.mockResolvedValue([option("gpt-6.1-sol", ["medium"])]);
  const onChange = vi.fn();
  render(<AgentExecutionFields workspaceId="room" value={initial} manager onChange={onChange} />);
  await screen.findByText(/A configuração atual não consta/);
  expect(screen.getByLabelText("Modelo")).toHaveValue("gpt-6-sol");
  expect(screen.getByLabelText("Raciocínio")).toHaveValue("ultra");
  expect(onChange).not.toHaveBeenCalled();
});

it("permite recuperar falha do catálogo sem perder os valores salvos", async () => {
  invoke.mockRejectedValueOnce("Login indisponível").mockResolvedValueOnce([option("gpt-6-sol", ["medium", "ultra"])]);
  render(<Editor />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Login indisponível");
  expect(screen.getByLabelText("Modelo")).toHaveValue("gpt-6-sol");
  fireEvent.click(screen.getByRole("button", { name: "Atualizar modelos" }));
  await waitFor(() => expect(screen.getByLabelText("Modelo")).toBeEnabled());
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByLabelText("Raciocínio")).toHaveValue("ultra");
});

it("ignora uma resposta atrasada de outro workspace", async () => {
  let finish: (models: CodexModel[]) => void = () => undefined;
  invoke.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; })).mockResolvedValueOnce([option("gpt-6.1-sol", ["medium"])]);
  const props = { value: initial, manager: true, onChange: vi.fn() };
  const { rerender } = render(<AgentExecutionFields {...props} workspaceId="first" />);
  rerender(<AgentExecutionFields {...props} workspaceId="second" />);
  await screen.findByRole("option", { name: "gpt-6.1-sol" });
  finish([option("old-model", ["low"])]);
  await waitFor(() => expect(screen.getByLabelText("Modelo")).toBeEnabled());
  expect(screen.queryByRole("option", { name: "old-model" })).not.toBeInTheDocument();
});
