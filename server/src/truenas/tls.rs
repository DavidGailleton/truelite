//! TLS to the NAS, trusted by certificate pinning.
//!
//! TrueNAS serves a self-signed certificate whose name rarely matches the address truelite
//! uses (LAN IP, Tailscale name). So we trust exactly one certificate: the one whose SHA-256
//! fingerprint (of the DER encoding) is in `TRUELITE_NAS_FINGERPRINT`. Like SSH host keys,
//! the pin *is* the trust anchor: CA chain, host name and expiry are not checked, and a
//! replaced certificate is refused until the pin is updated.
//!
//! The handshake signatures are still verified against the pinned certificate's public key,
//! so an attacker who copies the (public) certificate without its private key is refused.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, OtherError, SignatureScheme};

/// Command that prints the value to put in `TRUELITE_NAS_FINGERPRINT`.
const HOW_TO: &str = "get it with: openssl s_client -connect <nas>:443 </dev/null 2>/dev/null \
                      | openssl x509 -noout -fingerprint -sha256";

/// SHA-256 fingerprint of a DER-encoded certificate.
///
/// Parsed from 64 hex digits, case-insensitive, with optional `:` separators (the format of
/// `openssl x509 -fingerprint -sha256`). Displayed as uppercase colon-separated hex.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    /// Fingerprint of a DER-encoded certificate.
    pub fn of(cert_der: &[u8]) -> Self {
        let digest = ring::digest::digest(&ring::digest::SHA256, cert_der);
        let mut out = [0; 32];
        out.copy_from_slice(digest.as_ref());
        Self(out)
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(":")?;
            }
            write!(f, "{b:02X}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({self})")
    }
}

/// Why a string is not a SHA-256 fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FingerprintParseError {
    /// Not 64 hex digits once `:` separators are removed; holds the digit count found.
    WrongLength(usize),
    /// A character other than a hex digit or `:`.
    NotHex,
}

impl fmt::Display for FingerprintParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength(n) => write!(
                f,
                "a SHA-256 fingerprint has 64 hex digits, got {n}; {HOW_TO}"
            ),
            Self::NotHex => write!(
                f,
                "a SHA-256 fingerprint has only hex digits and ':'; {HOW_TO}"
            ),
        }
    }
}

impl std::error::Error for FingerprintParseError {}

impl FromStr for Fingerprint {
    type Err = FingerprintParseError;

    /// # Errors
    ///
    /// [`FingerprintParseError`] if `s` is not 64 hex digits (`:` separators allowed).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut out = [0u8; 32];
        let mut n = 0;
        for c in s.trim().chars().filter(|&c| c != ':') {
            let v = c.to_digit(16).ok_or(FingerprintParseError::NotHex)?;
            if let Some(byte) = out.get_mut(n / 2) {
                // `v` < 16, so the narrowing is lossless.
                *byte = (*byte << 4) | v as u8;
            }
            n += 1;
        }
        if n != 64 {
            return Err(FingerprintParseError::WrongLength(n));
        }
        Ok(Self(out))
    }
}

/// The NAS certificate is not the pinned one. Carried through rustls as
/// [`CertificateError::Other`] and turned into [`super::Error::CertMismatch`] by the client.
#[derive(Debug)]
pub(super) struct Mismatch {
    pub got: Fingerprint,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "certificate fingerprint {} is not the pinned one",
            self.got
        )
    }
}

impl std::error::Error for Mismatch {}

#[derive(Debug)]
struct PinnedVerifier {
    pin: Fingerprint,
    algs: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let got = Fingerprint::of(end_entity.as_ref());
        if got == self.pin {
            return Ok(ServerCertVerified::assertion());
        }
        tracing::warn!(got = %got, pinned = %self.pin, "NAS certificate does not match the pin");
        Err(rustls::Error::InvalidCertificate(CertificateError::Other(
            OtherError(Arc::new(Mismatch { got })),
        )))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

/// rustls client config that accepts only the certificate with fingerprint `pin`.
///
/// # Errors
///
/// A rustls error if the ring provider supports none of the default protocol versions
/// (cannot happen with the features we build with).
pub fn client_config(pin: Fingerprint) -> Result<Arc<rustls::ClientConfig>, rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = PinnedVerifier {
        pin,
        algs: provider.signature_verification_algorithms,
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()?
        // "dangerous" is rustls' name for any custom verifier; this one is stricter than
        // webpki for the single host it is used with.
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// Finds a pin mismatch inside a connect error (tokio-rustls reports handshake failures as an
/// `io::Error` wrapping the `rustls::Error`).
pub(super) fn mismatch_in(err: &tokio_tungstenite::tungstenite::Error) -> Option<Fingerprint> {
    use tokio_tungstenite::tungstenite::{Error as WsError, error::TlsError};
    let rustls_err = match err {
        WsError::Io(io) => io.get_ref()?.downcast_ref::<rustls::Error>()?,
        WsError::Tls(TlsError::Rustls(e)) => e.as_ref(),
        _ => return None,
    };
    let rustls::Error::InvalidCertificate(CertificateError::Other(other)) = rustls_err else {
        return None;
    };
    other.0.downcast_ref::<Mismatch>().map(|m| m.got)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_plain_colon_and_mixed_case_as_the_same_value() {
        let plain: Fingerprint = HEX.parse().unwrap();
        let colons: Fingerprint = plain.to_string().parse().unwrap();
        let upper: Fingerprint = HEX.to_uppercase().parse().unwrap();
        assert_eq!(plain, colons);
        assert_eq!(plain, upper);
        assert!(plain.to_string().starts_with("01:23:45:67:89:AB:CD:EF:"));
        assert_eq!(plain.to_string().len(), 64 + 31);
    }

    #[test]
    fn rejects_wrong_length_and_non_hex() {
        assert_eq!(
            HEX[..63].parse::<Fingerprint>(),
            Err(FingerprintParseError::WrongLength(63))
        );
        assert_eq!(
            format!("{HEX}0").parse::<Fingerprint>(),
            Err(FingerprintParseError::WrongLength(65))
        );
        assert_eq!(
            format!("{}g", &HEX[..63]).parse::<Fingerprint>(),
            Err(FingerprintParseError::NotHex)
        );
        assert_eq!(
            "".parse::<Fingerprint>(),
            Err(FingerprintParseError::WrongLength(0))
        );
        let msg = FingerprintParseError::NotHex.to_string();
        assert!(msg.contains("openssl"), "{msg}");
    }

    #[test]
    fn fingerprint_of_matches_sha256() {
        // SHA-256 of the empty input.
        let want: Fingerprint = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            .parse()
            .unwrap();
        assert_eq!(Fingerprint::of(b""), want);
    }
}
