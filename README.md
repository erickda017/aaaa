# Voz do Diabo

Alterador de voz **em tempo real** para mesas de RPG online (feito para a voz do Diabo de
*Ordem Paranormal*, mas todos os parâmetros são ajustáveis). Fica entre o seu microfone e o
Discord:

```
Microfone ──► [ Voz do Diabo ] ──► CABLE Input ═══► CABLE Output ──► Discord
                    │                 (cabo virtual)                 (seu "microfone")
                    └──► fones de ouvido (monitor opcional, para você se ouvir)
```

Escrito em Rust. O motor de áudio (`voice-core`) é independente de hardware e totalmente
testado; o app (`voice-app`) usa WASAPI (via `cpal`) e uma interface `egui`.

## Instalação (Windows)

1. **Rust**: https://rustup.rs (aceite instalar as *Visual Studio Build Tools* com C++ se pedir).
2. **VB-Cable** (cabo de áudio virtual, gratuito): https://vb-audio.com/Cable — instale e reinicie.
3. Compile e rode:

   ```powershell
   cargo run --release -p voice-app
   ```

   O executável fica em `target\release\voz-do-diabo.exe`.

## Configurar

**No app**

| Campo | Escolha |
|---|---|
| Microfone | seu microfone real |
| Saída | `CABLE Input (VB-Audio Virtual Cable)` (já vem pré-selecionado se o VB-Cable estiver instalado) |
| Monitor (opcional) | seus **fones** — assim você se ouve processado. Use fones, nunca caixas (microfonia) |

**No Discord** → Configurações → Voz e vídeo:

- Dispositivo de entrada: **`CABLE Output (VB-Audio Virtual Cable)`**
- **Desligue** Supressão de ruído (Krisp), Cancelamento de eco e Controle automático de ganho.
  Eles foram feitos para voz humana normal e estragam (ou cortam) a voz processada.
- Sensibilidade de entrada: manual, e baixa o bastante para não cortar o fim das falas.

## Usando na mesa

- **Atalho global** (padrão **F8**, funciona com o Discord/jogo em primeiro plano): liga e desliga o
  efeito. Dá para trocar a tecla ou capturar uma nova, inclusive os **botões laterais do mouse**.
- Modo **Segurar**: o efeito só fica ligado enquanto a tecla está pressionada (bom para entradas
  dramáticas). Modo **Alternar**: aperta uma vez liga, aperta de novo desliga.
- Ao desligar, sua voz normal passa direto (com *crossfade* de ~20 ms, sem estalo).

## Afinando o timbre

Comece pelos presets (**Diabo**, **Abissal**, **Sussurro**, **Sombrio**) e ajuste ouvindo pelo monitor.
Salve o que gostar em *Salvar ajustes atuais*.

- **Fale já num registro mais grave e devagar.** O processamento amplifica o que você dá a ele;
  quanto menor o deslocamento de tom necessário, mais natural (e menos "robótico") o resultado.
- **Ganho do microfone**: ajuste para os picos da sua fala ficarem por volta de **-12 dB** no
  medidor de entrada. A distorção e o compressor reagem ao nível.
- **Voz principal — tom** (semitons): -5 a -8 é o "grave humano-demoníaco"; -9 a -12 é abissal.
- **Camada grave** (uma oitava abaixo) dá o corpo; **Fantasma** (levemente desafinado) dá a
  sensação de várias vozes; **Sopro** e **Rugosidade** dão a parte sobrenatural.
- **Gate**: se o ruído do ambiente atravessa, suba o limiar até cortar só o silêncio.
- **Buffer de saída**: se houver estalos, aumente; se o atraso incomodar, diminua.
- **Tamanho do grão**: menor = menos atraso e mais granulado; maior = mais suave.

## Testar sem microfone

```powershell
# aplica todos os presets a uma voz sintética e mostra níveis / pitch resultante
cargo run --release -p voice-core --example render -- todos - saida\
# ou a uma gravação sua
cargo run --release -p voice-core --example render -- diabo minha_voz.wav saida\
```

## Estrutura

```
crates/voice-core   motor de DSP (sem hardware): pitch shifter síncrono ao período da voz (YIN),
                    camadas, saturação, EQ, reverb, gate, compressor, limiter, presets
crates/voice-app    app: cpal/WASAPI, ring buffer + reamostragem adaptativa, atalho global, UI
```

```powershell
cargo test --workspace     # 13 testes (pitch, bypass, limites, sincronização de relógios...)
```

## Estado atual e limitações conhecidas

- O DSP e a lógica de áudio foram validados por testes e simulação; **a primeira execução com
  dispositivos reais (WASAPI + VB-Cable) ainda precisa ser feita** — veja o roteiro abaixo.
- Os valores dos presets são um ponto de partida: o timbre só se acerta de ouvido.
- O pitch shifter muda os formantes junto com o tom (a voz soa "maior"). Não há controle de
  formante independente ainda.
- Atraso esperado: ~15 ms do efeito + buffer (35 ms padrão) + latência do Windows e do Discord.

## Roteiro do primeiro teste

1. `cargo run --release -p voice-app`; escolha microfone e saída `CABLE Input`; ative **monitor** com fones.
2. Clique **Iniciar**, fale: o medidor de **Entrada** deve mexer e o de **Saída** também.
3. Aperte **F8**: o selo vira "VOZ DO DIABO ATIVA" e você deve se ouvir grave nos fones.
4. Anote: há estalos? o atraso incomoda? (ajuste o *Buffer de saída*).
5. No Discord, entrada = `CABLE Output`, e peça a alguém para ouvir (ou use *Verificação de microfone*).
6. Afine os sliders e salve seu preset.
