# Alma formada pelo treinamento e separada da memória

## Comportamento entregue

1. A primeira conversa continua criando o perfil profissional inicial, sem mudar o nome.
2. O treinamento inicial consolida a alma: identidade, missão, responsabilidades,
   valores, comunicação, forma de trabalhar, critérios de decisão e limites ensinados.
3. O usuário abre **Alma e treinamento**, revisa a proposta e a memória e conclui a
   formação do gerente. O gerente treinado pode revisar e aprovar seus membros.
4. Memória registra aprendizados posteriores, preferências observadas, decisões e
   contexto útil. Tarefas e atribuições continuam guardando andamento e resultados.
5. Treinamento complementar explícito pode propor outra alma. A versão aprovada
   continua vigente até revisão; demandas normais não redefinem a alma.
6. O usuário também pode editar e aprovar a alma diretamente. Aprovar somente a alma
   não conclui automaticamente o treinamento.

## Contrato e persistência

- `TeamMember.soul` tem proposta com conteúdo, revisão e autor/data, além de
  `approved` com conteúdo, revisão, aprovador e data da última identidade aprovada.
- Limite: 12.000 caracteres Markdown. A memória mantém seu próprio limite de 16.000.
- `get_soul` consulta identidade; `get_memory` inclui alma, memória e treinamento.
  Agentes consultam a própria alma; o gerente pode consultar membros da mesma sala.
- `set_soul` escreve apenas a própria proposta durante formação, com revisão esperada.
  Não permite renomear, aprovar nem substituir implicitamente a identidade aprovada.
- `set_training_complete` exige `expectedRevision` da memória e
  `expectedSoulRevision` da alma. Aprovação e conclusão são persistidas juntas.
- Revisão recusa conversas em execução, pessoas pausadas, atribuições pendentes,
  versões desatualizadas e autoaprovação. Falhas de persistência da alma não alteram
  o estado em memória.
- A formação pode terminar com alma registrada e memória vazia. O término da conversa
  e a revisão continuam necessários para liberar trabalho.
- Sessões e próximas atribuições recebem a identidade aprovada; propostas pendentes
  aparecem com identificação explícita. O contrato atual é incluído também com
  templates antigos, sem sobrescrever arquivos personalizados das áreas.
- Cadastros antigos preservam treinamento, perfil, memória e histórico. Não há cópia
  automática desses textos para uma alma aprovada. Pessoas já liberadas continuam
  liberadas; a alma pode ser consolidada por edição ou treinamento complementar.
- Duplicar uma sala não copia almas. Mudar a especialidade exige nova alma e formação.

## Validação local

- 47 testes Rust passaram; 3 integrações opcionais com Codex ficaram ignoradas.
- 43 testes da interface passaram.
- TypeScript/Vite e o executável Tauri foram compilados.
- Cobertura inclui formação inicial só com alma, revisão de ambas as versões,
  aprovação humana do gerente, aprovação de membro pelo gerente, estabilidade durante
  demandas, isolamento entre salas/pessoas, propostas sem trocar identidade vigente,
  conflitos de edição, recarga, cadastros antigos, cópia da sala, mudança de função e
  falhas de persistência. O payload máximo com proposta, alma aprovada, perfil e
  memória cabe no transporte MCP local.

## Uso e limites

Reinicie o aplicativo recompilado com `npm run tauri dev` e reabra a sessão da pessoa.
Para alguém já treinado, use **Treinamento complementar**, ensine sua identidade e
responsabilidades e depois revise em **Alma e treinamento**. Também é possível
consolidar e aprovar o texto diretamente nessa tela.

A conversa completa com um agente no aplicativo aberto não foi validada nesta sessão;
o MCP do KanashaTerminal não estava disponível ao agente de desenvolvimento. Os testes
não substituem essa verificação de runtime. Não foi criado um campo de responsabilidade
coletiva do time, e nenhum cadastro existente recebeu aprovação por esta implementação.

Nenhuma skill foi utilizada nesta implementação.
