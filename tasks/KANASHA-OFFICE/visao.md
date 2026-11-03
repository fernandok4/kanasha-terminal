# Kanasha Office — proposta de evolução

Data: 2026-09-28. Este documento preserva a proposta inicial. A primeira versão foi implementada depois desta discussão; o funcionamento atual está no [README](../../README.md) e a validação em [entrega.md](entrega.md). As seções abaixo registram o planejamento original, não uma lista de pendências atuais.

## Decisões da conversa

- Área de Trabalho representa uma empresa.
- Workspace representa uma sala ou departamento, com um gerente como interlocutor principal.
- O usuário contrata os membros da equipe: nome, especialidade e orientações editáveis.
- DEV, QA, PO, Marketing e outras especialidades são pontos de partida. As responsabilidades podem mudar por tarefa, respeitando a competência de cada membro.
- O usuário inicialmente preferiu sessões próprias e depois delegou a decisão técnica ao implementador. A implementação adotou agentes gerenciados pelo KanashaTerminal, cada um com sessão Codex própria e terminal visível, coordenados pelo gerente.
- Codex é a prioridade. O produto deve funcionar com login do Codex via ChatGPT, sem exigir API key.
- Se necessário para preservar essa experiência, o usuário aceita avaliar subagentes nativos criados pelo gerente. Essa alternativa não substitui automaticamente a escolha por sessões próprias.

## Experiência proposta

1. Ao criar uma sala, o usuário nomeia seu gerente e define o objetivo da sala.
2. A sala pode começar apenas com o gerente. Contratar não inicia trabalho automaticamente.
3. Em “Contratar agente”, o usuário escolhe nome, especialidade e responsabilidades iniciais.
4. O usuário conversa com o gerente, que consulta a equipe disponível e atribui trabalhos dentro das orientações de cada membro.
5. A equipe mostra atividade atual, atribuição, resultado, impedimentos e última atualização por pessoa.
6. Selecionar uma pessoa abre sua sessão. O usuário pode conversar diretamente, ajustar orientações, interromper o trabalho ou retirar a pessoa da equipe.
7. O gerente recebe resultados identificados, consolida a tarefa da sala e apresenta a entrega ao usuário.

Exemplo: Marina gerencia a sala Produto. Rafael desenvolve, Lia valida e Nina esclarece requisitos. Rafael também pode investigar um problema; Lia pode ajudar a detalhar critérios. Marketing não recebe implementação de código automaticamente.

## Como preservar o funcionamento atual

O código atual inicia o executável `codex` em um PTY, exibe sua interface no xterm e injeta a configuração do MCP local do KanashaTerminal. A proposta mantém esse caminho de execução e deixa a autenticação sob responsabilidade do Codex.

O KanashaTerminal guarda a identidade da equipe e sua relação com as sessões. O Codex executa o trabalho. O gerente é uma sessão Codex com orientações de coordenação e acesso às ferramentas da equipe.

O MCP atual permite consultar e atualizar a tarefa da sala. Ele, sozinho, não entrega mensagens a uma sessão ociosa nem informa todos os eventos do Codex. A integração precisa também de um adaptador de sessões.

### Adaptador de sessões: etapa técnica a validar

O App Server do Codex é um candidato documentado: oferece threads, turnos, eventos, interrupção e autenticação com ChatGPT. O CLI instalado também anuncia `queue`, `agents`, `resume` e conexão a um servidor local. A presença desses comandos não comprova uma integração completa no KanashaTerminal.

Antes de implementar a equipe inteira, validar uma jornada pequena com duas sessões:

1. Iniciar uma sessão Codex visível usando a autenticação existente.
2. Obter seu identificador sem inferi-lo pelo texto do terminal ou pelo arquivo de sessão mais recente.
3. Vincular esse identificador ao membro e ao painel corretos.
4. Entregar uma atribuição identificada, confirmar recebimento e receber o resultado.
5. Interromper o turno correto e confirmar a interrupção por evento.
6. Reabrir a mesma sessão após reiniciar o aplicativo.

Essa validação ainda não foi executada. Não foi iniciado trabalho de agentes nem alterada a autenticação nesta investigação.

Não usar digitação cega no PTY para entregar atribuições: a entrada poderia chegar a um shell ou a outro estado da interface. Usar uma interface estruturada de sessão, validada para a versão instalada.

## Estado que falta no modelo atual

| Entidade | Dados propostos |
| --- | --- |
| Membro | ID estável, nome, especialidades, instruções, sala, gerente ou membro, vínculo ativo/inativo |
| Sessão | Membro, provedor, ID da sessão Codex, painel, estado da conexão |
| Atribuição | ID, autor, destinatário, objetivo, contexto, estado, resultado e referências |
| Evento | Sessão, atribuição, tipo, horário, origem e conteúdo relevante |

Separar contratação de execução: uma pessoa contratada pode estar sem sessão aberta. Separar também processo ativo de trabalho em andamento: um terminal aberto não prova que o agente está trabalhando.

Hoje o resumo e os steps pertencem ao Workspace; a atividade do agente é transitória por terminal. Cada membro precisa ter suas atribuições e resultados próprios, para que dois agentes não sobrescrevam o trabalho um do outro. O gerente consolida o resumo da sala.

## Ferramentas MCP propostas — ainda não disponíveis

Manter `get_task`, `get_task_steps` e `set_task`. Acrescentar, após definir os contratos:

- `get_team`: pessoas contratadas, especialidades e disponibilidade na sala.
- `get_agent`: atividade e atribuição de uma pessoa.
- `get_assignment`: objetivo, andamento e resultado de uma atribuição.
- `set_assignment`: registrar e entregar trabalho a um membro existente.
- `set_assignment_result`: registrar resultado ou impedimento do próprio trabalho.

Contratação e retirada de membros ficam sob controle do usuário na primeira versão. A identidade do chamador deve vir do vínculo de sessão; enviar um nome ou ID no argumento não concede autoridade. Um membro só altera suas atribuições; o gerente coordena os membros de sua sala.

`get_` e `set_` descrevem a intenção das tools. No MCP atual as chamadas usam JSON-RPC por STDIO e comunicação local; esses prefixos não transformam as tools em endpoints HTTP GET/PUT.

## Subagentes nativos, se adotados depois

Um membro contratado é uma identidade persistente do produto. Um subagente nativo é uma execução delegada pelo Codex. Para representá-los como a mesma pessoa, é necessário provar o vínculo entre os IDs, acompanhar eventos e preservar as ações de controle prometidas.

Instruir o gerente a limitar contratações em um prompt não equivale a impor esse limite no runtime. Se a experiência exigir limites rígidos para subagentes nativos, verificar suporte real antes de oferecer esse controle na interface.

## Entregas graduais

1. Validar a integração de duas sessões Codex com login existente, terminal visível, entrega e interrupção.
2. Implementar cadastro de gerente/equipe e vínculo estável com sessões, preservando os workspaces existentes.
3. Implementar atribuições, resultados e ferramentas MCP com isolamento por sala e membro.
4. Exibir atividade e histórico; recuperar vínculos após reinício; confirmar retirada de membros com trabalho ativo.

Os testes devem cobrir falhas reais: mensagem duplicada não inicia trabalho duas vezes; sessão errada não recebe atribuição; membro não altera o resultado de outro; reconexão não apresenta atividade antiga como atual. Validar também uma jornada manual com o Codex autenticado, pois mocks não comprovam essa compatibilidade.

## Referências verificadas

- [Autenticação oficial do Codex](https://learn.chatgpt.com/docs/auth): login com ChatGPT para acesso por assinatura e alternativa por API key.
- [Codex App Server](https://learn.chatgpt.com/docs/app-server): interface de sessões, turnos, eventos e login com ChatGPT.
- [Subagentes do Codex](https://learn.chatgpt.com/docs/agent-configuration/subagents): delegação e configuração de subagentes.
- Código local: `src-tauri/src/lib.rs`, `src-tauri/src/models.rs`, `src-tauri/src/mcp.rs`, `src/App.tsx`.

Sem API key não significa uso ilimitado: a execução continua sujeita ao acesso e aos limites da conta Codex/ChatGPT.
