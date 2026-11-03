# Nova conversa no terminal do gerente — 2026-11-03

Implementado no checkout local, preservando alterações preexistentes.

O cabeçalho do terminal do gerente oferece um balão de conversa com sinal de mais ao lado de Encerrar processo. O botão mantém rótulo acessível e tooltip Nova conversa. Não aparece em terminais comuns ou de funcionários.

Usa reset_agent_context, o mesmo fluxo da tela Equipe, sem alterações no backend. Preserva as regras atuais: pausa, turno, pergunta e atribuição em execução ou incerta impedem o reset. Durante a requisição, bloqueia cliques duplicados. Após erro, mantém o requestId para repetição idempotente; após sucesso, o próximo clique usa outro ID. Erros são mostrados pela mensagem já existente no aplicativo.

Validação local: 47 testes passaram em App.test.tsx e TerminalPanel.test.tsx, incluindo localização e visibilidade do botão, turno ativo, bloqueio de clique duplicado e reaproveitamento do ID após erro. Após a troca do texto pelo SVG, npm run build passou; git diff --check passou. Nenhuma sessão real do gerente foi reiniciada durante a validação. Sem revisor independente ou integração MCP Kanasha disponível nesta sessão.
