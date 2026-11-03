# Nome preservado na primeira conversa

O nome exibido desde a criação do gerente ou contratação de uma pessoa é o nome
cadastrado. Criar o perfil profissional deve preservá-lo. O usuário pode editar o
nome pela interface.

## Causa e correção

O cadastro `automaticName: true` era tratado como provisório. As instruções
orientavam o Codex a escolher outro nome, e `set_professional_profile` gravava essa
mudança no título do painel. Isso permitiu a troca Beatriz → Marina durante uma
apresentação.

Agora a ferramenta rejeita um nome diferente do título cadastrado e não escreve no
título. A regra também vale para cadastros antigos e nomes copiados ao duplicar
Workspaces. As instruções da sessão, o template v6 da área, o contrato MCP, a
interface e o README refletem essa regra. O perfil continua sendo criado na primeira
conversa; memória e revisão do treinamento mantêm seus contratos anteriores.

## Verificação local

- Rust: 41 testes passaram; 3 testes de integração com Codex ficaram ignorados.
- Equipe: 8 testes passaram.
- Build TypeScript/Vite passou.
- A regressão cobre gerente Beatriz com `automaticName: true`, contratado com nome
  automático, rejeição de renomeação sem alterar estado em memória ou disco e
  criação válida com nome preservado após recarregar. Memória e estado de treinamento
  são preservados.

## Aplicação da mudança

É necessário executar o aplicativo recompilado. Reabrir a sessão da pessoa carrega
as novas instruções geradas. Arquivos personalizados de outras áreas não são
sobrescritos; mesmo com instruções antigas, a ferramenta atualizada bloqueia a troca.
Nomes já alterados não foram revertidos. A integração MCP do KanashaTerminal não
estava disponível na sessão de desenvolvimento, e a conversa na aplicação aberta
não foi validada ao vivo.

Nenhuma skill foi utilizada nesta correção.
