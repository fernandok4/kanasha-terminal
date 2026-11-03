# Tipos de workspace — 2026-11-03

Implementado no checkout `/Users/fernando.kanashiro/Workspace/kanasha-terminal`, base 5b8ed9e, com alterações locais preexistentes e sem commit novo.

- Organização normal é o padrão: terminais livres, sem gerente automático e sem menu Equipe.
- Equipe é uma escolha explícita ao criar workspace, incluindo o primeiro workspace de uma área. Cria gerente e permite contratar pessoas.
- O tipo é persistido e preservado ao duplicar. Arquivos antigos com membros são carregados como equipes; sem membros, como normais. IDs, painéis e estado existente são preservados.
- Escritório apresenta apenas equipes. Trocar da tela Equipe para uma organização normal retorna aos terminais sem remontar as sessões.
- Contratação em organização normal é recusada pelo backend.

## Evidências locais

- `npm test`: exit 0, 81 testes em 10 arquivos. Inclui escolha dos tipos, menu condicional, mapa filtrado e preservação das instâncias dos terminais.
- `cargo test --lib`: exit 0, 77 testes aprovados, 5 integrações ignoradas. Inclui criação, bloqueio de contratação, duplicação, reload e arquivo legado sem o campo de tipo.
- `npm run build`: exit 0. Aviso de tamanho do bundle permanece.
- `cargo build`: exit 0; executável debug atualizado.
- `cargo fmt --check` e `git diff --check`: exit 0.

## Limites

Sem integração MCP Kanasha disponível e sem revisor independente disponível nesta sessão. Validação de interface por testes automatizados; não houve conferência manual no aplicativo aberto. A aplicação em execução precisa ser reaberta para carregar o backend recompilado; sessões de outras pessoas não foram interrompidas.
