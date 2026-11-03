import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AreaInstructions } from "./AreaInstructions";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
afterEach(cleanup);
const document = { path: "/empresa/.kanasha/AGENTS.md", content: "# Empresa\nRegras próprias", revision: "revision-original" };
beforeEach(() => { invoke.mockReset().mockResolvedValue(document); });

it("edita as instruções da área com revisão e explica a aplicação nas sessões", async () => {
  const onSaved = vi.fn();
  render(<AreaInstructions areaId="area" areaName="Empresa" onClose={vi.fn()} onSaved={onSaved} />);
  const editor = await screen.findByLabelText("Instruções em Markdown");
  expect(editor).toHaveValue(document.content);
  fireEvent.change(editor, { target: { value: "# Novas instruções" } });
  invoke.mockResolvedValue({ ...document, content: "# Novas instruções", revision: "revision-new" });
  fireEvent.click(screen.getByRole("button", { name: "Salvar instruções" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_area_instructions", { areaId: "area", content: "# Novas instruções", expectedRevision: "revision-original" }));
  expect(await screen.findByRole("status")).toHaveTextContent("Encerre e reabra as sessões");
  expect(onSaved).toHaveBeenCalledOnce();
});

it("preserva o rascunho quando o arquivo mudou fora do editor", async () => {
  const onSaved = vi.fn();
  render(<AreaInstructions areaId="area" areaName="Empresa" onClose={vi.fn()} onSaved={onSaved} />);
  const editor = await screen.findByLabelText("Instruções em Markdown");
  fireEvent.change(editor, { target: { value: "# Meu rascunho" } });
  invoke.mockRejectedValue("As instruções foram alteradas fora deste editor.");
  fireEvent.click(screen.getByRole("button", { name: "Salvar instruções" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("fora deste editor");
  expect(editor).toHaveValue("# Meu rascunho");
  expect(onSaved).not.toHaveBeenCalled();
});
