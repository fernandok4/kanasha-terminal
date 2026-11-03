import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { TaskMarkdown } from "./TaskMarkdown";

const { openUrl } = vi.hoisted(() => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl }));
beforeEach(() => openUrl.mockReset().mockResolvedValue(undefined));
afterEach(cleanup);

it("organiza tarefas, entregas, testes e links navegáveis em Markdown", async () => {
  render(<TaskMarkdown content={"## Tarefas envolvidas\n\n### X — Cadastro\nPermitir corrigir o nome.\n\n### Y — Busca\nEncontrar o cadastro corrigido.\n\n## O que foi implementado\n\n- **X:** edição do nome.\n- **Y:** busca atualizada.\n\n## Testes e resultados\n\n| Ambiente | Resultado |\n| --- | --- |\n| DEV | Busca passou |\n\n## Links de acompanhamento\n\n[Jira X](https://jira.example.test/browse/X)\n\n[Raposa MR](https://raposa.example.test/project/-/merge_requests/1)\n\nGMUD: link não informado."} />);
  expect(screen.getByRole("heading", { name: "X — Cadastro" })).toBeInTheDocument();
  expect(screen.getAllByRole("listitem")).toHaveLength(2);
  expect(screen.getByRole("table")).toHaveTextContent("Busca passou");
  fireEvent.click(screen.getByRole("link", { name: "Raposa MR" }));
  expect(openUrl).toHaveBeenCalledWith("https://raposa.example.test/project/-/merge_requests/1");
  openUrl.mockRejectedValueOnce(new Error("Falha"));
  fireEvent.click(screen.getByRole("link", { name: "Jira X" }));
  await screen.findByRole("alert");
});

it("mantém texto legado e impede HTML, imagens e links executáveis ou locais", async () => {
  const { container, rerender } = render(<TaskMarkdown content={"Resumo existente.\nSegunda linha."} />);
  expect(container.textContent).toBe("Resumo existente.\nSegunda linha.");
  rerender(<TaskMarkdown content={'<script>alert(1)</script>\n\n<iframe src="https://example.test"></iframe>\n\n![Imagem](https://example.test/a.png)\n\n[Executar](javascript:alert%281%29) [Arquivo](file:///tmp/test) [Relativo](/local) [Site](https://example.test)'} />);
  expect(container.querySelector("script, iframe, img")).toBeNull();
  expect(screen.getAllByRole("link")).toHaveLength(1);
  fireEvent.click(screen.getByText("Executar"));
  fireEvent.click(screen.getByText("Arquivo"));
  await waitFor(() => expect(openUrl).not.toHaveBeenCalled());
});
