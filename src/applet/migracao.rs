//! Quem vem do Status Hub original não perde fixados, ordem nem aparência: na
//! primeira subida as chaves do id antigo são copiadas para o novo, e só as que o
//! novo ainda não tem. Nada é apagado no id antigo.

use std::path::{Path, PathBuf};

pub const ID_ANTIGO: &str = "io.github.marcelogomes90.cosmic-ext-applet-status-hub";

const CHAVES: [&str; 4] = ["pinned", "order", "colour-icons", "hub-side"];

/// Pastas onde a versão antiga pode ter gravado: a nativa e a do flatpak.
fn origens(config: &Path, home: Option<&Path>, versao: u64) -> Vec<PathBuf> {
    let mut origens = vec![
        config
            .join("cosmic")
            .join(ID_ANTIGO)
            .join(format!("v{versao}")),
    ];
    if let Some(home) = home {
        origens.push(
            home.join(".var/app")
                .join(ID_ANTIGO)
                .join("config/cosmic")
                .join(ID_ANTIGO)
                .join(format!("v{versao}")),
        );
    }
    origens
}

/// Devolve quantas chaves foram trazidas.
pub fn migrar_de(config: &Path, home: Option<&Path>, novo: &str, versao: u64) -> usize {
    let destino = config.join("cosmic").join(novo).join(format!("v{versao}"));
    let mut trazidas = 0;

    for chave in CHAVES {
        let alvo = destino.join(chave);
        if alvo.exists() {
            continue;
        }
        let Some(fonte) = origens(config, home, versao)
            .into_iter()
            .map(|origem| origem.join(chave))
            .find(|fonte| fonte.is_file())
        else {
            continue;
        };
        if let Err(erro) = std::fs::create_dir_all(&destino)
            .and_then(|()| std::fs::copy(&fonte, &alvo).map(|_| ()))
        {
            tracing::warn!(%erro, chave, "não deu para trazer a preferência do Status Hub");
            continue;
        }
        trazidas += 1;
    }

    if trazidas > 0 {
        tracing::info!(
            trazidas,
            "preferências do Status Hub trazidas para a Bandeja"
        );
    }
    trazidas
}

pub fn migrar(novo: &str, versao: u64) -> usize {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(config) = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".config")))
    else {
        return 0;
    };
    migrar_de(&config, home.as_deref(), novo, versao)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta(nome: &str) -> PathBuf {
        let pasta = std::env::temp_dir().join(format!(
            "meowsystem-bandeja-migracao-{}-{nome}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&pasta);
        pasta
    }

    fn gravar(caminho: &Path, texto: &str) {
        std::fs::create_dir_all(caminho.parent().unwrap()).unwrap();
        std::fs::write(caminho, texto).unwrap();
    }

    #[test]
    fn traz_as_chaves_antigas_e_nao_pisa_nas_novas() {
        let base = pasta("nativa");
        let config = base.join("config");
        let antigo = config.join("cosmic").join(ID_ANTIGO).join("v1");
        let novo = config.join("cosmic/io.github.meowsystem.Bandeja/v1");
        gravar(&antigo.join("order"), "[\"a\"]");
        gravar(&antigo.join("pinned"), "[\"b\"]");
        gravar(&novo.join("pinned"), "[\"novo\"]");

        let trazidas = migrar_de(&config, None, "io.github.meowsystem.Bandeja", 1);

        assert_eq!(trazidas, 1);
        assert_eq!(
            std::fs::read_to_string(novo.join("order")).unwrap(),
            "[\"a\"]"
        );
        assert_eq!(
            std::fs::read_to_string(novo.join("pinned")).unwrap(),
            "[\"novo\"]"
        );
        assert!(antigo.join("order").exists(), "o id antigo fica intacto");
        assert_eq!(
            migrar_de(&config, None, "io.github.meowsystem.Bandeja", 1),
            0
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn traz_tambem_do_flatpak() {
        let base = pasta("flatpak");
        let config = base.join("config");
        let home = base.join("home");
        gravar(
            &home
                .join(".var/app")
                .join(ID_ANTIGO)
                .join("config/cosmic")
                .join(ID_ANTIGO)
                .join("v1/hub-side"),
            "Left",
        );

        assert_eq!(
            migrar_de(&config, Some(&home), "io.github.meowsystem.Bandeja", 1),
            1
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn sem_versao_antiga_nao_cria_nada() {
        let base = pasta("vazia");
        assert_eq!(migrar_de(&base, None, "io.github.meowsystem.Bandeja", 1), 0);
        assert!(!base.exists());
    }
}
