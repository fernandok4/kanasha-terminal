# Resumo descritivo e organizado

## Mudanças

- `summary` continua sendo Markdown de até 8.000 caracteres; não há migração de schema.
- O MCP orienta seis seções na inicialização e na descrição de `set_task.summary`:
  visão geral, tarefas envolvidas, implementação, testes, pendências e links.
- O template v5 de `.kanasha/AGENTS.md` detalha a escrita em linguagem de produto,
  com um subtítulo por tarefa e distinção entre pedido, entrega, validação e publicação.
- A tela renderiza Markdown com títulos, listas, tabelas e links HTTP(S) pelo opener
  do Tauri. Não carrega imagens nem executa HTML. Resumos legados são preservados.
- Renderer: [react-markdown](https://github.com/remarkjs/react-markdown), com remark-gfm;
  integração e comportamento de abertura de links cobertos por testes locais.

## Validação

- 20 testes React: App e TaskMarkdown; preservação da tela de terminais, conteúdo
  legado, estrutura Markdown, links externos, falha do opener e conteúdo não permitido.
- Cinco testes MCP: inicialização, tools, consultas separadas e contrato de atualização.
- Prévia inspecionada em Chromium: seis seções, sem overflow horizontal em larguras
  de 1.100 e 420 pixels. Captura em `preview.png`.
- Build TypeScript/Vite e pacote macOS concluídos com sucesso.

## Aplicação e limites

Novas sessões no aplicativo atualizado recebem as instruções MCP. O texto do resumo
é escrito pelo agente com base no contexto e nas evidências; a interface não inventa
explicações nem busca Jira, Raposa ou GMUD automaticamente. O MCP não estava disponível
na sessão de desenvolvimento; nenhum resumo de outro Workspace foi alterado.

Arquivos `.kanasha/AGENTS.md` personalizados de outras áreas continuam preservados;
o template v5 é o padrão para novas áreas. O exemplo em `exemplo.md` reorganiza somente
o conteúdo da captura do usuário, marcando as lacunas. Não foi publicado no Workspace
nem representa consulta de status ou nova validação das B2Cs.
