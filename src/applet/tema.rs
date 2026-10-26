//! Extensão de tema: cores e ícones que um theme pack entrega ao applet.
//!
//! O arquivo é `$XDG_CONFIG_HOME/meowsystem/extensoes/<APP_ID>/tema.conf`, texto
//! `chave = valor`. Ausente, ilegível ou com linha ruim, o applet segue o tema do
//! COSMIC: a extensão só acrescenta, nunca é condição para subir.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const ARQUIVO: &str = "tema.conf";
pub const VARIAVEL: &str = "MEOWSYSTEM_TEMA";
pub const PREFIXO_ICONE: &str = "icone.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Cor {
    /// Só `#rrggbb` e `#rrggbbaa`: quem traduz nome de paleta para hex é o MeowSystem.
    pub fn ler(texto: &str) -> Option<Self> {
        let hex = texto.trim().strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let canal = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self {
            r: canal(0)?,
            g: canal(2)?,
            b: canal(4)?,
            a: if hex.len() == 8 { canal(6)? } else { 255 },
        })
    }

    pub fn rgb(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }

    pub fn iced(self) -> cosmic::iced::Color {
        cosmic::iced::Color::from_rgba8(self.r, self.g, self.b, f32::from(self.a) / 255.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tema {
    /// Glifos simbólicos e ícones recoloridos (painel e popup).
    pub tinta: Option<Cor>,
    /// Fundo do cartão do popup.
    pub fundo: Option<Cor>,
    /// Contorno do cartão do popup.
    pub borda: Option<Cor>,
    /// Texto do popup.
    pub texto: Option<Cor>,
    /// Fundo do item cujo menu está aberto.
    pub destaque: Option<Cor>,
    /// Ícones dos títulos de seção nas configurações.
    pub acento: Option<Cor>,
    /// Nome de ícone do tema para o botão do painel.
    pub icone_botao: Option<String>,
    /// `Id` do item da bandeja -> nome de ícone do tema.
    pub icones: BTreeMap<String, String>,
}

impl Tema {
    pub fn vazio(&self) -> bool {
        self == &Self::default()
    }

    /// Lê o texto inteiro; devolve o tema e um aviso por linha recusada.
    pub fn ler(texto: &str) -> (Self, Vec<String>) {
        let mut tema = Self::default();
        let mut avisos = Vec::new();

        for (numero, linha) in texto.lines().enumerate() {
            let linha = linha.trim();
            if linha.is_empty() || linha.starts_with('#') {
                continue;
            }
            let Some((chave, valor)) = linha.split_once('=') else {
                avisos.push(format!("linha {}: falta o «=»", numero + 1));
                continue;
            };
            let (chave, valor) = (chave.trim(), valor.trim());
            if let Err(motivo) = tema.aplicar(chave, valor) {
                avisos.push(format!("linha {}: {motivo}", numero + 1));
            }
        }

        (tema, avisos)
    }

    fn aplicar(&mut self, chave: &str, valor: &str) -> Result<(), String> {
        let cor = |valor: &str| {
            Cor::ler(valor).ok_or_else(|| format!("«{chave}» quer #rrggbb, veio «{valor}»"))
        };
        let nome = |valor: &str| {
            nome_de_icone(valor)
                .then(|| valor.to_owned())
                .ok_or_else(|| format!("«{chave}» quer um nome de ícone, veio «{valor}»"))
        };

        match chave {
            "tinta" => self.tinta = Some(cor(valor)?),
            "fundo" => self.fundo = Some(cor(valor)?),
            "borda" => self.borda = Some(cor(valor)?),
            "texto" => self.texto = Some(cor(valor)?),
            "destaque" => self.destaque = Some(cor(valor)?),
            "acento" => self.acento = Some(cor(valor)?),
            "icone-botao" => self.icone_botao = Some(nome(valor)?),
            _ => match chave.strip_prefix(PREFIXO_ICONE) {
                Some(id) if !id.is_empty() => {
                    self.icones.insert(id.to_owned(), nome(valor)?);
                }
                _ => return Err(format!("chave desconhecida «{chave}»")),
            },
        }
        Ok(())
    }

    pub fn icone_do_item(&self, id: &str) -> Option<&str> {
        self.icones.get(id).map(String::as_str)
    }
}

/// Nome de ícone do tema, não caminho: `/` e espaço deixariam o pack apontar arquivo.
fn nome_de_icone(valor: &str) -> bool {
    !valor.is_empty()
        && valor
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// As cores da extensão já convertidas, lidas no desenho: trocar o tema não pede
/// reconstruir widget nenhum, só o próximo quadro.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Paleta {
    pub tinta: Option<cosmic::iced::Color>,
    pub fundo: Option<cosmic::iced::Color>,
    pub borda: Option<cosmic::iced::Color>,
    pub texto: Option<cosmic::iced::Color>,
    pub destaque: Option<cosmic::iced::Color>,
    pub acento: Option<cosmic::iced::Color>,
}

impl Paleta {
    pub fn de(tema: &Tema) -> Self {
        Self {
            tinta: tema.tinta.map(Cor::iced),
            fundo: tema.fundo.map(Cor::iced),
            borda: tema.borda.map(Cor::iced),
            texto: tema.texto.map(Cor::iced),
            destaque: tema.destaque.map(Cor::iced),
            acento: tema.acento.map(Cor::iced),
        }
    }
}

thread_local! {
    static PALETA: std::cell::Cell<Paleta> = std::cell::Cell::new(Paleta::default());
}

/// Vale para a thread da interface, que é a única que desenha.
pub fn vestir(tema: &Tema) {
    PALETA.with(|paleta| paleta.set(Paleta::de(tema)));
}

pub fn paleta() -> Paleta {
    PALETA.with(std::cell::Cell::get)
}

pub fn caminho_padrao(app_id: &str) -> Option<PathBuf> {
    if let Some(caminho) = std::env::var_os(VARIAVEL).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(caminho));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("meowsystem/extensoes").join(app_id).join(ARQUIVO))
}

pub fn carregar_de(caminho: &Path) -> Tema {
    let texto = match std::fs::read_to_string(caminho) {
        Ok(texto) => texto,
        Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => return Tema::default(),
        Err(erro) => {
            tracing::warn!(caminho = %caminho.display(), %erro, "extensão de tema ilegível");
            return Tema::default();
        }
    };
    let (tema, avisos) = Tema::ler(&texto);
    for aviso in avisos {
        tracing::warn!(caminho = %caminho.display(), "extensão de tema: {aviso}");
    }
    tema
}

pub fn carregar(app_id: &str) -> Tema {
    caminho_padrao(app_id)
        .map(|caminho| carregar_de(&caminho))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cor_aceita_seis_e_oito_digitos_e_recusa_o_resto() {
        assert_eq!(
            Cor::ler("#cba6f7"),
            Some(Cor {
                r: 0xcb,
                g: 0xa6,
                b: 0xf7,
                a: 255
            })
        );
        assert_eq!(Cor::ler(" #1e1e2e80 ").map(|c| c.a), Some(0x80));
        for ruim in ["cba6f7", "#cba6f", "#cba6f7a", "#gggggg", "mauve", "", "#"] {
            assert_eq!(Cor::ler(ruim), None, "{ruim:?} passou");
        }
    }

    #[test]
    fn um_arquivo_completo_vira_um_tema_completo() {
        let (tema, avisos) = Tema::ler(
            "# Catppuccin Mocha\n\
             tinta = #cdd6f4\n\
             fundo=#1e1e2e\n\
             borda = #45475a\n\
             texto = #cdd6f4\n\
             destaque = #313244\n\
             acento = #cba6f7\n\
             icone-botao = view-app-grid-symbolic\n\
             icone.spotify-client = spotify-indicator\n\
             icone.hefesto-dualsense4unix = input-gaming-symbolic\n",
        );

        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(tema.fundo.map(Cor::rgb), Some([0x1e, 0x1e, 0x2e]));
        assert_eq!(tema.acento.map(Cor::rgb), Some([0xcb, 0xa6, 0xf7]));
        assert_eq!(tema.icone_botao.as_deref(), Some("view-app-grid-symbolic"));
        assert_eq!(
            tema.icone_do_item("spotify-client"),
            Some("spotify-indicator")
        );
        assert_eq!(tema.icone_do_item("steam"), None);
    }

    #[test]
    fn linha_ruim_vira_aviso_e_nao_leva_as_boas_junto() {
        let (tema, avisos) = Tema::ler(
            "fundo = mauve\n\
             sem igual\n\
             cor-nova = #ffffff\n\
             icone.x = ../../etc/passwd\n\
             icone. = algo\n\
             tinta = #ffffff\n",
        );

        assert_eq!(avisos.len(), 5, "{avisos:?}");
        assert_eq!(tema.fundo, None);
        assert!(tema.icones.is_empty());
        assert_eq!(tema.tinta.map(Cor::rgb), Some([255, 255, 255]));
    }

    #[test]
    fn arquivo_ausente_e_o_tema_do_cosmic() {
        let caminho = std::env::temp_dir().join(format!(
            "meowsystem-bandeja-sem-tema-{}/tema.conf",
            std::process::id()
        ));
        assert!(carregar_de(&caminho).vazio());
    }

    #[test]
    fn arquivo_binario_nao_derruba() {
        let pasta = std::env::temp_dir().join(format!(
            "meowsystem-bandeja-tema-binario-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&pasta).unwrap();
        let caminho = pasta.join(ARQUIVO);
        std::fs::write(&caminho, [0xff, 0xfe, 0x00, 0x9f]).unwrap();

        assert!(carregar_de(&caminho).vazio());
        let _ = std::fs::remove_dir_all(pasta);
    }

    #[test]
    fn vestir_troca_a_paleta_e_um_tema_vazio_a_devolve_ao_cosmic() {
        let (tema, _) = Tema::ler("fundo = #1e1e2e\nacento = #cba6f7\n");
        vestir(&tema);
        assert!(paleta().fundo.is_some());
        assert!(paleta().acento.is_some());
        assert!(paleta().tinta.is_none());

        vestir(&Tema::default());
        assert_eq!(paleta(), Paleta::default());
    }

    #[test]
    fn o_caminho_mora_na_pasta_de_extensoes_do_meowsystem() {
        let caminho = caminho_padrao("io.github.meowsystem.Bandeja").unwrap();
        if std::env::var_os(VARIAVEL).is_none() {
            assert!(
                caminho.ends_with("meowsystem/extensoes/io.github.meowsystem.Bandeja/tema.conf")
            );
        }
    }
}
