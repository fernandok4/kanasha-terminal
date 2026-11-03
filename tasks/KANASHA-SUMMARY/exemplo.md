## Visão geral

Este trabalho reúne **B2C-4047, B2C-4048 e B2C-4049** em uma entrega conjunta para melhorar o controle de recorrências no Minhas Economias. O motivo técnico para reuni-las não foi detalhado no registro disponível.

## Tarefas envolvidas

### B2C-4047 — Alteração e histórico da recorrência

Trata da edição, do corte e do encerramento de uma recorrência, além de seu histórico. O resumo anterior não detalha quais lançamentos são afetados por cada operação; esse comportamento precisa ser explicado com base na tarefa.

### B2C-4048 — Correção manual de vínculos

Permite corrigir manualmente um vínculo e protege essa correção contra uma associação automática posterior. Falta identificar no registro quais itens são vinculados e em quais situações a proteção atua.

### B2C-4049 — Prazo da recorrência

Trata do prazo e da data de término da recorrência. As regras específicas para aplicar essa data ainda precisam ser descritas com base na tarefa.

## O que foi implementado

- O registro informa uma correção para **lançamentos parcelados**, identificada tecnicamente como `INSTALLMENT`.
- O escopo aprovado inclui os projetos financial-core-webservice, financial-connection-integration e kanasha-flow.
- O registro não discrimina quais mudanças foram concluídas em cada tarefa. Essa parte precisa ser complementada pelo agente responsável.

## Testes e resultados

- **DEV:** o registro informa validação da correção de lançamentos parcelados, com 110 verificações principais e 21 verificações de limpeza dos dados de teste, sem falhas.
- Ainda faltam a descrição dos cenários, a data da execução e a referência da evidência. As contagens acima não representam uma nova execução de testes.

## Pendências e próximos passos

- A validação da reimportação pelo Open Finance aguarda dados de teste utilizáveis. O registro não detalha quais dados faltam.
- As MRs Core !899, FCI !111 e QA !22 foram registradas como rascunho. Esse é o último estado informado, sem nova consulta.
- Completar as mudanças por tarefa, os cenários testados e os links abaixo.

## Links de acompanhamento

- **Jira:** links não informados para B2C-4047, B2C-4048 e B2C-4049.
- **Raposa:** URLs não informadas para Core !899, FCI !111 e QA !22.
- **GMUD:** criação e links não informados no registro disponível.
