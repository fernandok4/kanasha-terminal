# Filas de tarefas e steps por funcionário

Entrega local de 2026-09-30. O usuário confirmou que limpar significa trocar a
conversa, preservando alma, lembranças e arquivos. Esclareceu que tarefas são
códigos do Jira e que analisar, planejar e implementar são steps da mesma tarefa.

## Comportamento

1. Cada sala mantém tarefas locais por `taskCode`, com estado, resumo, steps e
   revisão próprios. O registro geral `workspace.task` continua existindo.
2. O gerente registra a tarefa e seus steps por `set_task {taskCode,...}`.
   `get_tasks` lista as tarefas; `get_task` e `get_task_steps` aceitam `taskCode`.
   Sem código, os contratos de leitura e escrita antigos permanecem válidos.
3. A atribuição liga `taskCode`, `stepIds`, funcionário destinatário e `requestId`.
   Steps são IDs reais da tarefa, independentes dos títulos. A aplicação rejeita
   steps inexistentes, IDs repetidos, steps sem taskCode, alterações do recibo e
   remoção de IDs já referenciados pelo histórico.
4. Cada funcionário possui `taskQueue` persistente. A primeira tarefa permanece
   atual até sua conclusão. Dentro dela, as atribuições executam uma por vez na
   ordem de entrada. Um step da atual acrescentado depois da próxima tarefa ainda
   executa antes dela. Sem steps disponíveis, aguarda o gerente; não alterna tarefas.
5. O gerente revisa resultados, atualiza os steps e conclui a tarefa com
   `set_task {taskCode,status:"done"}`. A aplicação rejeita conclusão enquanto há
   steps não concluídos ou atribuições de funcionários queued/running/uncertain.
   Concluir uma atribuição não conclui a tarefa automaticamente. A coordenação do
   próprio gerente pode finalizar a tarefa após verificar as entregas dos membros.
6. A conclusão remove o código das filas pendentes dos funcionários e acorda as
   sessões pelo MCP. Trabalho novo em tarefa encerrada exige reabertura explícita;
   a nova atribuição inclui a tarefa novamente ao fim da fila. Ordens circulares
   entre filas compartilhadas são rejeitadas antes de salvar a atribuição.
7. Na troca de código, a aplicação prepara uma thread Codex nova, com nome, perfil,
   alma, memória e instruções atuais. Todos os steps do mesmo código conservam a
   conversa. O gerente conserva sua própria conversa de coordenação.
8. Sem taskCode, atribuições antigas e novas conservam contexto por padrão.
   `freshContext:true` permite demanda independente sem código. Com código, a
   fronteira é a tarefa; o sinalizador não limpa steps da mesma tarefa.
9. `reset_context` permite ao gerente treinado uma nova conversa de outro funcionário
   da sala, após concluir sua tarefa atual. A interface oferece **Nova conversa**
   também para o gerente, por ação do usuário. Turnos, perguntas pendentes, resultados
   incertos e pessoas pausadas bloqueiam uma troca nova. Recibos de troca persistem.

## Persistência e sessões

- A troca não usa `close_member_session`: essa operação cancela trabalho. Salva o
  checkpoint de thread nova antes de desmontar conexão e PTY, mantendo jobs, taskQueue,
  aprendizagem, treinamento e arquivos. Registra o ID da thread anterior no recibo;
  cada atribuição executada registra a thread utilizada.
- Repetir um recibo não cria outra conversa. Recibos manuais e de atribuições têm
  namespaces separados. Falha de persistência não altera estado nem revoga tokens.
- Eventos e saída atrasados da conexão anterior são filtrados pela geração. O evento
  de nova conversa é emitido antes de liberar a abertura do PTY seguinte. O frontend
  recria xterm no tamanho inicial e guarda a saída antecipada para reproduzi-la.
- Falha antes do primeiro turno preserva a fila para tentativa explícita. Execução
  incerta não é reenviada automaticamente. Pausas continuam sendo decisão do usuário.
- Salvar taskCode e stepIds não altera issues no Jira: trata-se do vínculo local
  entre as tarefas recebidas, funcionários e atribuições do KanashaTerminal.

## Interface e instruções

- **Tarefas e responsáveis**, **Steps por tarefa**, fila de tarefas com filas internas
  e seleção de steps na atribuição. Código da tarefa é um campo próprio.
- `get_team`/`get_agent` expõem fila de tarefas, atribuições da tarefa, execução atual
  e vínculos. `get_task_steps {taskCode}` expõe responsáveis e atribuições por step.
- Template v10 de `.kanasha/AGENTS.md` e contrato técnico injetado orientam o gerente
  a reutilizar funcionários, enfileirar mesmo ocupados, terminar a atual primeiro e
  preservar contexto entre steps. Instruções personalizadas de outras áreas não são
  sobrescritas pela atualização do aplicativo.
- `/clear` digitado diretamente no Codex continua sem reconciliação automática de
  thread pela aplicação; use **Nova conversa** ou a troca automática de tarefa.

## Validação

- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 59 testes passaram;
  quatro testes externos ficam ignorados na suíte padrão.
- `npm test`: 48 testes passaram, incluindo vínculos de steps, agrupamento de tarefa,
  recibos após resposta incerta e reconstrução do terminal com saída antecipada.
- `npm run build`, `cargo build`, `cargo fmt -- --check` e `git diff --check`: passaram.
  Vite conserva o aviso existente de tamanho de bundle.
- Teste externo executado separadamente:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib installed_codex_context_reset -- --ignored --nocapture`.
  Passou com Codex instalado, sem inferência: nova thread e conexão distintas,
  instruções contendo alma/memória e retomada do histórico anterior preservado.
  Codex 0.159.2 usa histórico paginado; a verificação lê o rollout indicado por
  `thread/read`, pois mensagens injetadas não aparecem como turnos de inferência.
- `tools/list` pelo modo `--mcp` do binário compilado confirmou 14 ferramentas,
  incluindo `get_tasks`, `stepIds` e `reset_context`. Isso verifica o catálogo do
  binário; não substitui chamada MCP à aplicação aberta.
- Esta sessão não dispõe do MCP do aplicativo aberto. A jornada completa de gerente
  delegando duas tarefas reais pela interface/MCP com inferência não foi exercitada.
  Nenhuma sessão ou fila real do usuário foi interrompida para esses testes.

## Disponibilidade

Código e builds locais preparados. É necessário abrir a versão atualizada do app
para carregar o backend e as novas tools; a instalação distribuída não foi alterada.
