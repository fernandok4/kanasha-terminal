import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { DeliveryHistory } from "./DeliveryHistory";
import type { Assignment, Delivery } from "../types";

afterEach(cleanup);
const assignment: Assignment = { id: "work", taskCode: "B2C-4423", message: "Investigar", status: "done", result: "Concluído", assignedBy: "manager", createdAt: 1, updatedAt: 2, turnId: null };
const delivery: Delivery = { revision: 1, submittedBy: "davi", submittedAt: 2, declaration: { kind: "investigation", summary: "A MR não comprova alteração de Davi.", limitations: "Checkout compartilhado.", contextIds: [], evidence: [{ id: "mr", kind: "merge_request", reference: "https://example.invalid/mr/96", version: "old-sha", details: "Referência relatada pelo funcionário." }] }, repositories: [], reviews: [] };

it("não transforma execução concluída sem entrega em ausência de trabalho", () => {
  render(<DeliveryHistory assignment={assignment} />);
  expect(screen.getByText(/Entrega estruturada não registrada/)).toHaveTextContent("Isso não comprova ausência de trabalho");
});
it("mostra MR declarada sem apresentá-la como verificada", () => {
  render(<DeliveryHistory assignment={{ ...assignment, deliveries: [delivery] }} />);
  expect(screen.getByText("Entrega v1 · Aguardando conferência")).toBeVisible();
  expect(screen.getByText("MR · Declarada, não conferida")).toBeVisible();
  expect(screen.getByRole("link")).toHaveAttribute("href", "https://example.invalid/mr/96");
});
it("preserva indisponibilidade de verificação mesmo com aceite e nova versão volta a aguardar", () => {
  const reviewed: Delivery = { ...delivery, reviews: [{ revision: 1, reviewedBy: "manager", reviewedAt: 3, assessment: { deliveryRevision: 1, decision: "accepted", summary: "Investigação aceita com limitação de acesso.", checks: [{ evidenceId: "mr", state: "unavailable", source: "GitLab MCP", reference: "https://example.invalid/mr/96", version: "", note: "Autenticação expirada." }] } }] };
  const { rerender } = render(<DeliveryHistory assignment={{ ...assignment, deliveries: [reviewed] }} />);
  expect(screen.getByText("Entrega v1 · Aceita pelo gerente")).toBeVisible();
  expect(screen.getByText("MR · Verificação indisponível")).toBeVisible();
  rerender(<DeliveryHistory assignment={{ ...assignment, deliveries: [reviewed, { ...delivery, revision: 2, submittedAt: 4 }] }} />);
  expect(screen.getByText("Entrega v2 · Aguardando conferência")).toBeVisible();
  const latest = screen.getByText("Entrega v2 · Aguardando conferência").closest("details")!;
  expect(within(latest).getByText("MR · Declarada, não conferida")).toBeVisible();
  expect(screen.getByText(/Entrega v1 · Aceita pelo gerente · Versão anterior/)).toBeInTheDocument();
});
it("não cria link executável a partir de referência declarada", () => {
  const unsafe = { ...delivery, declaration: { ...delivery.declaration, evidence: [{ ...delivery.declaration.evidence[0], reference: "javascript:alert(1)" }] } };
  render(<DeliveryHistory assignment={{ ...assignment, deliveries: [unsafe] }} />);
  expect(screen.queryByRole("link")).not.toBeInTheDocument();
  expect(screen.getByText("javascript:alert(1)")).toBeVisible();
});
