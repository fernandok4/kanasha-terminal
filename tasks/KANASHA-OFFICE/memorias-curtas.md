# Memórias individuais com experiências curtas e referências

O template padrão `.kanasha/AGENTS.md` foi atualizado para v8. Cada funcionário,
inclusive o gerente, registra suas próprias experiências e aprendizados em lembranças
de uma a três frases, com data explícita, origem, resultado confirmado e pendências.
Treinamentos também são experiências: registrar que aconteceram é diferente de
afirmar que foram aprovados. A alma continua guardando identidade e responsabilidades.

Quando forem necessários mais detalhes, a memória aponta para documentos existentes
da tarefa ou arquivos pessoais em `equipes/<workspace-id>/memorias/<terminal-id>/`,
sob a raiz da Área. O caminho usa os IDs estáveis da sessão, não o nome da pessoa.
Os arquivos devem existir e ser conferidos antes de registrar o link. O agente consulta
as referências pertinentes ao assunto para recuperar contexto, sem carregar todos os
documentos em cada conversa. A organização por pessoa não altera o compartilhamento
da pasta da Área.

As instruções sugerem destacar lembranças recentes e resumir as antigas, mantendo
referências, decisões válidas e pendências. Não há seleção automática por idade nesta
mudança. A aplicação continua persistindo um texto de memória por pessoa, e
`set_memory` substitui esse texto; a orientação exige preservar lembranças válidas.

A orientação foi alinhada com as instruções geradas para as sessões, a descrição de
`set_memory` no MCP e o README. Os arquivos personalizados de outras áreas continuam
preservados. O contrato fornecido pelo app também orienta registrar brevemente a
experiência do treinamento, sem copiar a alma inteira para a memória.

Validação: quatro testes existentes de instruções e preservação dos arquivos do
usuário, compilação do executável Tauri, formatação Rust e `git diff --check`.
Não foram criadas lembranças fictícias nem alteradas memórias dos funcionários.
O comportamento de registro pelo agente não foi validado numa conversa ao vivo.

Para aplicar ao runtime, use o app recompilado e reabra a sessão do funcionário.
O template v8 completo é usado em novas áreas; arquivos de áreas existentes não
são sobrescritos automaticamente. Nenhuma skill foi utilizada nesta alteração.
