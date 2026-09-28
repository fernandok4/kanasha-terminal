# KanashaTerminal

Aplicativo desktop local para organizar agentes de terminal em paralelo. A interface está em PT-BR e funciona em Linux e macOS por meio de Tauri, Rust e WebView.

## O que o MVP oferece

- Áreas de Trabalho vinculadas a uma pasta-raiz.
- Workspaces em abas dentro de cada Área.
- Resumo persistente por Workspace e status individual dos agentes, atualizados via MCP local.
- Vários terminais em painéis com divisões horizontais ou verticais.
- Divisores arrastáveis e ajustáveis por teclado, com proporções restauradas localmente.
- Foco rápido entre painéis, maximização temporária, nomes e etiquetas visíveis para identificar cada agente.
- Perfis internos para Shell, Codex, Claude e Gemini, quando disponíveis no `PATH`, além de perfis customizados.
- Processos que continuam ativos ao trocar de Área ou Workspace.
- Restauração local de Áreas, Workspaces, layouts, painéis e metadados de perfis. Terminais restaurados ficam parados e podem ser iniciados manualmente.
- Menus de contexto, atalhos de teclado, tooltips e confirmação antes de encerrar processos.

O terminal sempre inicia na pasta-raiz da Área. Os dados ficam somente no diretório de configuração do aplicativo e não há servidor HTTP, WebSocket ou porta de rede. A integração MCP usa STDIO e um socket Unix local protegido, disponível apenas durante a execução do aplicativo.

## Privacidade da persistência

O aplicativo persiste o resumo da tarefa informado pelo agente como parte dos metadados do Workspace. O status individual dos agentes existe somente durante a execução atual. Ele **não persiste** buffers ou saída de terminal, comandos executados, argumentos de perfil, tokens, segredos, variáveis de ambiente, PIDs ou sessões. Para proteger essa fronteira, o executável e os argumentos de um perfil customizado são mantidos somente em memória: devem ser informados novamente após reiniciar o aplicativo.

## Resumo da tarefa via MCP

O botão **Resumo**, ao lado de **+ Terminal**, abre uma tela própria sem desmontar os terminais do Workspace. Além do status da tarefa, a tela mostra cada terminal e seu estado de agente: `Disponível`, `Trabalhando`, `Aguardando`, `Bloqueado`, `Concluído` ou `Desconectado`. A cor da aba do Workspace indica a situação agregada, priorizando `Trabalhando` quando pelo menos um agente está executando uma etapa.

Ao iniciar o perfil **Codex**, o KanashaTerminal configura automaticamente o MCP local no processo. O servidor expõe somente a ferramenta `task_state`: sem argumentos ela lê o contexto compartilhado e o estado do próprio agente; com argumentos ela altera apenas os campos enviados. A resposta de uma atualização é somente `ok`, e o Codex limita a saída da ferramenta a 512 tokens.

Os perfis de agentes sempre iniciam em modo irrestrito, usando a opção própria de cada CLI: `--dangerously-bypass-approvals-and-sandbox` no Codex, `--dangerously-skip-permissions` no Claude e `--approval-mode=yolo` no Gemini. A mesma regra é aplicada a perfis customizados cujo executável seja `codex`, `claude` ou `gemini`, sem duplicar uma opção equivalente já configurada. O perfil Shell e outros executáveis customizados mantêm os argumentos informados, pois não existe uma opção universal de modo irrestrito para comandos arbitrários.

Os campos compartilhados são `status`, `summary`, `current`, `next` e `blocker`. Os campos individuais são `agentStatus` e `agentCurrent`; o terminal é identificado automaticamente, sem enviar IDs na chamada. O agente reporta somente transições e mudanças relevantes de etapa, sem heartbeat periódico, para reduzir o consumo de tokens. Encerrar o processo remove seu estado ativo sem concluir a tarefa automaticamente.

O executável também aceita `--mcp` para uso com outros clientes MCP STDIO. Os terminais recebem `KANASHA_MCP_SOCKET`, `KANASHA_MCP_TOKEN`, `KANASHA_WORKSPACE_ID` e `KANASHA_TERMINAL_ID`; cada cliente precisa encaminhar essas variáveis ao servidor conforme sua própria configuração.

## Pré-requisitos

O projeto requer Node.js `^20.19.0` ou `>=22.12.0`, com npm, e o Rust instalado pelo `rustup`.

### macOS

Para desenvolvimento desktop, instale as ferramentas de linha de comando do Xcode:

```bash
xcode-select --install
```

Instale o Rust e abra um novo terminal para que `cargo` seja adicionado ao `PATH`:

```bash
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

Se quiser continuar no terminal atual logo após a instalação:

```bash
source "$HOME/.cargo/env"
```

### Ubuntu/Debian

Instale as dependências nativas do Tauri:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Depois instale o Rust e abra um novo terminal:

```bash
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

## Desenvolvimento

Os mesmos comandos iniciam o projeto no macOS e no Ubuntu/Debian:

```bash
npm install
npm run tauri dev
```

Se aparecer `failed to run 'cargo metadata' ... No such file or directory`, o Rust ainda não está instalado ou o terminal atual ainda não carregou `~/.cargo/bin` no `PATH`. Abra outro terminal ou execute `source "$HOME/.cargo/env"`.

## Instalador para macOS

O DMG precisa ser gerado em um Mac. A arquitetura do instalador será a mesma da máquina usada no build (Apple Silicon ou Intel):

```bash
npm install
npm run tauri build -- --bundles app,dmg
```

Os artefatos ficam em:

- aplicativo: `src-tauri/target/release/bundle/macos/KanashaTerminal.app`
- instalador: `src-tauri/target/release/bundle/dmg/*.dmg`

Abra o DMG e arraste `KanashaTerminal` para `Applications`. O build local recebe uma assinatura ad hoc, mas não é publicado, notarizado nem assinado com um certificado Apple Developer. Ao distribuir o arquivo para outro Mac, o Gatekeeper pode exigir autorização em **Ajustes do Sistema > Privacidade e Segurança**; distribuição pública requer assinatura e notarização.

Se o build parar em `Running bundle_dmg.sh`, permita que o aplicativo de terminal usado no build controle o Finder em **Ajustes do Sistema > Privacidade e Segurança > Automação** e execute o comando novamente.

## Validação

```bash
npm test
npm run build
cd src-tauri && cargo fmt --check && cargo test
npm run tauri build
```

O último comando gera, localmente, os pacotes compatíveis com o sistema em que o build foi executado, dentro de `src-tauri/target/release/bundle/`. Ele não publica, assina com certificado de distribuição ou faz deploy.

## Atalhos

- `Ctrl/Cmd + N`: cria um novo terminal no workspace atual.
- `Ctrl/Cmd + Shift + N`: abre outro terminal mesmo quando um terminal está com foco.
- `Ctrl/Cmd + Shift + W`: cria um novo workspace na Área atual.
- `Ctrl/Cmd + ,`: abre a criação de perfil customizado.
- `Ctrl/Cmd + Shift + A`: abre a criação de Área de Trabalho.
- `Ctrl/Cmd + Alt + ←/↑` e `Ctrl/Cmd + Alt + →/↓`: muda o foco para o painel anterior/próximo, inclusive a partir do terminal.
- `Ctrl/Cmd + Shift + C` e `Ctrl/Cmd + Shift + V`: copiam a seleção e colam texto no terminal, pelos controles nativos do xterm/WebView.

Os atalhos são locais à janela. Os de criação não interceptam a entrada do terminal quando o painel está com foco; a navegação entre painéis foi deliberadamente mantida disponível nesse contexto.
