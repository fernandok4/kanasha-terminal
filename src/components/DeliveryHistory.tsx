import type { Assignment, DeliveryGitSnapshot } from "../types";

const decisions = { accepted: "Aceita pelo gerente", changes_requested: "Correções solicitadas", inconclusive: "Conferência inconclusiva" };
const checks = { verified: "Conferida pelo gerente", unavailable: "Verificação indisponível", divergent: "Divergência encontrada", not_checked: "Não conferida" };
const kinds = { code_change: "Alteração de código", investigation: "Investigação", validation: "Validação", documentation: "Documentação" };
const evidenceKinds = { code: "Código", gitlab: "GitLab", merge_request: "MR", test: "Teste", review: "Revisão", deployment: "Deploy", investigation: "Investigação" };
const date = (timestamp: number) => new Date(timestamp * 1000).toLocaleString("pt-BR");

function Reference({ value }: { value: string }) {
  return /^https?:\/\//i.test(value) ? <a href={value} target="_blank" rel="noreferrer">{value}</a> : <span>{value || "Não informada"}</span>;
}
function GitObservation({ title, value }: { title: string; value: DeliveryGitSnapshot }) {
  return <details><summary>{title} · {date(value.observedAt)}</summary>
    <p>Checkout: <code>{value.checkoutPath}</code></p>
    <p>Repositório: {value.repositoryRoot ?? "Indisponível"} · Branch: {value.branch ?? "Sem branch identificada"}</p>
    <p>SHA observado: <code>{value.head ?? "Indisponível"}</code></p>
    <p>Impressão do diff rastreado: <code>{value.trackedDiffSha256 ?? "Indisponível"}</code></p>
    <p>Impressão do diff staged: <code>{value.stagedDiffSha256 ?? "Indisponível"}</code></p>
    {value.status.length ? <pre>{value.status.join("\n")}</pre> : <p>Nenhuma entrada de alteração coletada; confira as limitações abaixo.</p>}
    {value.limitations.map((limitation, index) => <p className="team-muted" key={index}>{limitation}</p>)}
  </details>;
}

export function DeliveryHistory({ assignment }: { assignment: Assignment }) {
  const deliveries = assignment.deliveries ?? [];
  const contexts = assignment.workContexts ?? [];
  return <section className="delivery-history" aria-label={`Entregas da atribuição ${assignment.id}`}>
    {!deliveries.length ? <p className="team-muted">Entrega estruturada não registrada. Isso não comprova ausência de trabalho.</p> : null}
    {contexts.length ? <details><summary>Checkouts registrados ({contexts.length})</summary>{contexts.map(context => <div key={context.id}>
      <p>{context.request.note}</p>
      <GitObservation title="Git no registro do checkout" value={context.baseline} />
    </div>)}<p className="team-muted">O registro é uma observação daquele momento; não reconstrói o início da atribuição nem comprova autoria.</p></details> : null}
    {[...deliveries].reverse().map((delivery, index) => {
      const review = delivery.reviews[delivery.reviews.length - 1];
      return <details key={delivery.revision} open={index === 0}>
        <summary>Entrega v{delivery.revision} · {review ? decisions[review.assessment.decision] : "Aguardando conferência"}{index > 0 ? " · Versão anterior" : ""}</summary>
        <p>{kinds[delivery.declaration.kind]} · {date(delivery.submittedAt)}</p>
        <p>{delivery.declaration.summary}</p>
        {delivery.declaration.limitations ? <p>Limitações declaradas: {delivery.declaration.limitations}</p> : null}
        <p className="team-muted">As referências abaixo são declaradas pelo funcionário. Cada evidência tem sua própria conferência.</p>
        {delivery.declaration.evidence.map(evidence => {
          const check = review?.assessment.checks.find(c => c.evidenceId === evidence.id);
          return <div key={evidence.id}><strong>{evidenceKinds[evidence.kind]} · {check ? checks[check.state] : "Declarada, não conferida"}</strong>
            <p><Reference value={evidence.reference} /> · Versão: <code>{evidence.version || "Não informada"}</code></p>
            <p>{evidence.details}</p>
            {check ? <p>Conferência: {check.note} · Fonte: {check.source || "Não informada"} · <Reference value={check.reference} /> · Versão conferida: {check.version || "Não informada"}</p> : null}
          </div>;
        })}
        {delivery.repositories.map(repo => <div key={repo.contextId}>
          <GitObservation title="Git na entrega" value={repo.finalSnapshot} />
          <details><summary>Comparação desde o registro do checkout</summary>
            {repo.comparison.commits.length ? <pre>{repo.comparison.commits.join("\n")}</pre> : <p>Nenhum commit listado nesta comparação.</p>}
            {repo.comparison.committedFiles.length ? <pre>{repo.comparison.committedFiles.join("\n")}</pre> : null}
            {repo.comparison.limitations.map((limitation, i) => <p className="team-muted" key={i}>{limitation}</p>)}
          </details>
        </div>)}
        {delivery.reviews.length ? <details><summary>Histórico de conferências ({delivery.reviews.length})</summary>{delivery.reviews.map(item => <div key={item.revision}>
          <p>Conferência {item.revision} · {decisions[item.assessment.decision]} · {date(item.reviewedAt)}</p>
          <p>{item.assessment.summary}</p>
          {item.assessment.checks.map(check => <p key={check.evidenceId}>{check.evidenceId}: {checks[check.state]} · {check.note} · Fonte: {check.source || "Não informada"} · <Reference value={check.reference} /> · {check.version}</p>)}
        </div>)}</details> : null}
      </details>;
    })}
  </section>;
}
