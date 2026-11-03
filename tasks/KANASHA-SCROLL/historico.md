# Corrupção visual do histórico — 2026-09-28

## Relato

Captura do Codex com trechos deslocados, linhas misturadas e texto sobreposto.
O gatilho exato não foi informado. A captura sozinha não permite afirmar uma causa única.

## Falhas corrigidas

1. **Geometria durante a inicialização:** o PTY começa com 120 colunas e 36 linhas.
   O frontend ajustava o xterm ao painel antes de interpretar a saída inicial já
   recebida. Comandos de cursor emitidos para a geometria original atingiam outra
   linha quando essa saída quebrava em uma largura menor. Agora o xterm começa
   em 120×36; a primeira medição e o ResizeObserver são ativados somente após o
   parser consumir a fila inicial. O callback é cancelado logicamente ao desmontar.
2. **Quebras de linha:** `convertEol: true` acrescentava um retorno à coluna zero
   aos LF do processo. Agora os comandos CR/LF são respeitados como enviados pelo
   PTY. O terminal não deve reinterpretar os comandos de posicionamento da TUI.
3. **UTF-8 fragmentado:** cada leitura do PTY era convertida separadamente com
   `from_utf8_lossy`, podendo transformar um único caractere em vários símbolos
   de substituição e alterar a largura ocupada. O decodificador agora conserva
   bytes incompletos entre leituras. Bytes inválidos continuam tendo substituição;
   um caractere incompleto no encerramento é liberado antes do evento de saída.

## Evidência

- Componente TerminalPanel real, React, xterm 6 e FitAddon em Chromium, simulando
  apenas o transporte Tauri. Painel de 500 pixels resultou em 66 colunas.
  Saída inicial: 119 letras `A`, CR/LF, `B`, subir uma linha, CR, `OK`.
  Antes: primeira linha permanecia `AAAA…` (a atualização atingia outra linha).
  Depois: primeira linha `OKAA…`, preservando a posição original do redesenho.
- No mesmo componente, `ABC` + LF + `X`: antes, `X` na coluna zero da segunda
  linha; depois, `X` na coluna três, respeitando a semântica de LF sem CR.
- Regressão de scroll: três eventos de mouse encaminhados no gesto simulado,
  tanto inicialmente quanto após cinco ciclos de ocultar/exibir com saída ativa.
- 21 testes React de TerminalPanel/App passaram. Cobrem também a barreira de
  inicialização, cancelamento após desmontagem, modos de mouse e navegação.
- Quatro testes Rust de terminal passaram: dois de PTY real e dois de UTF-8.
  A sequência com acentos, japonês, emoji e comandos ANSI foi dividida em cada
  posição de byte possível e também entregue byte a byte, sem corrupção.

Comandos: `npm test -- src/components/TerminalPanel.test.tsx src/App.test.tsx` e
`cargo test --manifest-path src-tauri/Cargo.toml --lib -j 1 terminal::tests`.

## Limites e aplicação

Reproduzimos falhas de posicionamento capazes de causar corrupção visual; não
reproduzimos a conversa exata da captura nem validamos o trackpad físico no
WKWebView. As alterações não reconstroem um buffer já corrompido. É preciso abrir
o aplicativo atualizado e retomar a conversa do Codex para reconstruir sua tela.
Nenhum processo do usuário foi encerrado automaticamente.

O texto `Hook failed / code 127` pertence à saída do processo e não foi tratado
como causa comprovada da corrupção. Não foram alterados hooks ou configurações
globais do Codex.
