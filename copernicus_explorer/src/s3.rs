use std::env;
use std::path::{Path, PathBuf};

use s3::Region;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use tokio::io::AsyncRead;

use crate::error::{CopernicusError, Result};

/// S3 connection parameters resolved from config file or environment.
#[derive(Debug, Clone)]
pub struct S3Config {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: String,
    pub endpoint: String,
}

impl S3Config {
    /// Resolve S3 credentials following the priority chain:
    ///
    /// 1. Explicit config file path (from `--s3-config` flag)
    /// 2. Default config at `~/.config/copernicus_explorer/s3.conf`
    /// 3. `S3_ACCESS_KEY_ID` / `S3_SECRET_ACCESS_KEY` / `S3_ENDPOINT` / `S3_REGION`
    /// 4. `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_ENDPOINT_URL` / `AWS_REGION`
    ///
    /// When reading from an INI file, `bucket` is used to select the
    /// matching `[section]`.  If no section matches the bucket name, the
    /// file is skipped and resolution continues with environment variables.
    pub fn resolve(bucket: &str, config_path: Option<&Path>) -> Result<Self> {
        if let Some(path) = config_path
            && let Some(cfg) = Self::from_ini_file(path, bucket)?
        {
            return Ok(cfg);
        }

        if let Some(home) = dirs::home_dir() {
            let default_path = home
                .join(".config")
                .join("copernicus_explorer")
                .join("s3.conf");
            if default_path.exists()
                && let Some(cfg) = Self::from_ini_file(&default_path, bucket)?
            {
                return Ok(cfg);
            }
        }

        if let Some(cfg) = Self::from_env_prefix(
            "S3_ACCESS_KEY_ID",
            "S3_SECRET_ACCESS_KEY",
            "S3_ENDPOINT",
            "S3_REGION",
        ) {
            return Ok(cfg);
        }

        if let Some(cfg) = Self::from_env_prefix(
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_ENDPOINT_URL",
            "AWS_REGION",
        ) {
            return Ok(cfg);
        }

        Err(CopernicusError::S3Error(
            "no S3 credentials found: provide --s3-config, \
             place a config at ~/.config/copernicus_explorer/s3.conf, \
             or set S3_*/AWS_* environment variables"
                .into(),
        ))
    }

    /// Parse an rclone-style INI file, selecting the section whose name
    /// matches `bucket`.  Returns `Ok(None)` when the file is valid but
    /// contains no section matching the bucket name, so the caller can
    /// continue down the fallback chain.
    fn from_ini_file(path: &Path, bucket: &str) -> Result<Option<Self>> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            CopernicusError::S3Error(format!("cannot read S3 config {}: {e}", path.display()))
        })?;

        let sections = parse_ini_sections(&content);

        let section = match sections.iter().find(|s| s.name == bucket) {
            Some(s) => s,
            None => return Ok(None),
        };

        let access_key_id = section.get("access_key_id").ok_or_else(|| {
            CopernicusError::S3Error(format!(
                "missing 'access_key_id' in [{}] of {}",
                section.name,
                path.display()
            ))
        })?;
        let secret_access_key = section.get("secret_access_key").ok_or_else(|| {
            CopernicusError::S3Error(format!(
                "missing 'secret_access_key' in [{}] of {}",
                section.name,
                path.display()
            ))
        })?;
        let region = section.get("region").ok_or_else(|| {
            CopernicusError::S3Error(format!(
                "missing 'region' in [{}] of {}",
                section.name,
                path.display()
            ))
        })?;
        let endpoint = section.get("endpoint").ok_or_else(|| {
            CopernicusError::S3Error(format!(
                "missing 'endpoint' in [{}] of {}",
                section.name,
                path.display()
            ))
        })?;

        Ok(Some(Self {
            access_key_id: access_key_id.to_string(),
            secret_access_key: secret_access_key.to_string(),
            region: region.to_string(),
            endpoint: endpoint.to_string(),
        }))
    }

    fn from_env_prefix(ak_var: &str, sk_var: &str, ep_var: &str, rg_var: &str) -> Option<Self> {
        let access_key_id = env::var(ak_var).ok()?;
        let secret_access_key = env::var(sk_var).ok()?;
        let endpoint = env::var(ep_var).ok()?;
        let region = env::var(rg_var).ok()?;
        Some(Self {
            access_key_id,
            secret_access_key,
            region,
            endpoint,
        })
    }

    fn to_bucket(&self, bucket_name: &str) -> Result<Box<Bucket>> {
        let region = Region::Custom {
            region: self.region.clone(),
            endpoint: self.endpoint.clone(),
        };
        let credentials = Credentials::new(
            Some(&self.access_key_id),
            Some(&self.secret_access_key),
            None,
            None,
            None,
        )
        .map_err(|e| CopernicusError::S3Error(format!("invalid S3 credentials: {e}")))?;

        Bucket::new(bucket_name, region, credentials).map_err(|e| {
            CopernicusError::S3Error(format!("failed to create S3 bucket handle: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// INI parser helpers
// ---------------------------------------------------------------------------

struct IniSection {
    name: String,
    entries: Vec<(String, String)>,
}

impl IniSection {
    fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Parse an rclone-style INI file into a list of named sections.
/// Lines before the first `[section]` header are ignored.
fn parse_ini_sections(content: &str) -> Vec<IniSection> {
    let mut sections = Vec::new();
    let mut current: Option<IniSection> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            if let Some(sec) = current.take() {
                sections.push(sec);
            }
            let name = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
            current = Some(IniSection {
                name,
                entries: Vec::new(),
            });
            continue;
        }
        if let Some(ref mut sec) = current
            && let Some((key, value)) = line.split_once('=')
        {
            sec.entries
                .push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    if let Some(sec) = current {
        sections.push(sec);
    }

    sections
}

// ---------------------------------------------------------------------------
// S3 destination & output destination
// ---------------------------------------------------------------------------

/// Parsed S3 destination from an `s3://bucket/prefix/` URI.
#[derive(Debug, Clone)]
pub struct S3Destination {
    pub bucket: String,
    pub prefix: String,
    pub config: S3Config,
}

impl S3Destination {
    /// Upload bytes from an `AsyncRead` source to the S3 destination.
    pub async fn upload<R: AsyncRead + Unpin>(
        &self,
        reader: &mut R,
        filename: &str,
    ) -> Result<String> {
        let bucket = self.config.to_bucket(&self.bucket)?;

        let s3_key = if self.prefix.is_empty() {
            filename.to_string()
        } else {
            format!("{}/{filename}", self.prefix.trim_end_matches('/'))
        };

        bucket
            .put_object_stream(reader, &s3_key)
            .await
            .map_err(|e| CopernicusError::S3Error(format!("S3 upload failed: {e}")))?;

        Ok(format!("s3://{}/{s3_key}", self.bucket))
    }
}

/// Where to write downloaded products.
#[derive(Debug, Clone)]
pub enum OutputDestination {
    Local(PathBuf),
    S3(S3Destination),
}

/// Parse the output directory string into either a local path or an S3 destination.
///
/// If `output` starts with `s3://`, the S3 credential resolution chain is
/// triggered using `s3_config_path`.
pub fn parse_output_destination(
    output: &str,
    s3_config_path: Option<&Path>,
) -> Result<OutputDestination> {
    if output.starts_with("s3:") && !output.starts_with("s3://") {
        return Err(CopernicusError::S3Error(format!(
            "malformed S3 URI '{output}': expected s3://bucket/prefix/"
        )));
    }

    if let Some(rest) = output.strip_prefix("s3://") {
        let (bucket, prefix) = match rest.find('/') {
            Some(idx) => (rest[..idx].to_string(), rest[idx + 1..].to_string()),
            None => (rest.to_string(), String::new()),
        };

        if bucket.is_empty() {
            return Err(CopernicusError::S3Error(
                "S3 URI must include a bucket name: s3://bucket/prefix/".into(),
            ));
        }

        let config = S3Config::resolve(&bucket, s3_config_path)?;

        Ok(OutputDestination::S3(S3Destination {
            bucket,
            prefix,
            config,
        }))
    } else {
        Ok(OutputDestination::Local(PathBuf::from(output)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_ini_selects_matching_section() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            "[bucket-a]\n\
             type = s3\n\
             access_key_id = AK_A\n\
             secret_access_key = SK_A\n\
             region = eu-west-1\n\
             endpoint = https://s3.a.example.com\n\
             \n\
             [bucket-b]\n\
             type = s3\n\
             access_key_id = AK_B\n\
             secret_access_key = SK_B\n\
             region = us-east-1\n\
             endpoint = https://s3.b.example.com"
        )
        .unwrap();

        let cfg = S3Config::from_ini_file(tmp.path(), "bucket-b")
            .unwrap()
            .expect("section bucket-b should be found");
        assert_eq!(cfg.access_key_id, "AK_B");
        assert_eq!(cfg.secret_access_key, "SK_B");
        assert_eq!(cfg.region, "us-east-1");
        assert_eq!(cfg.endpoint, "https://s3.b.example.com");
    }

    #[test]
    fn parse_ini_returns_none_when_no_section_matches() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            "[my-remote]\n\
             type = s3\n\
             access_key_id = AKID\n\
             secret_access_key = SKEY\n\
             region = eu-west-1\n\
             endpoint = https://s3.example.com"
        )
        .unwrap();

        let result = S3Config::from_ini_file(tmp.path(), "unknown-bucket").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_s3_uri_with_prefix() {
        // Bypass credential resolution by testing the URI parsing directly
        let output = "s3://my-bucket/some/prefix/";
        let rest = output.strip_prefix("s3://").unwrap();
        let (bucket, prefix) = match rest.find('/') {
            Some(idx) => (rest[..idx].to_string(), rest[idx + 1..].to_string()),
            None => (rest.to_string(), String::new()),
        };
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "some/prefix/");
    }

    #[test]
    fn parse_s3_uri_no_prefix() {
        let output = "s3://my-bucket";
        let rest = output.strip_prefix("s3://").unwrap();
        let (bucket, prefix) = match rest.find('/') {
            Some(idx) => (rest[..idx].to_string(), rest[idx + 1..].to_string()),
            None => (rest.to_string(), String::new()),
        };
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "");
    }

    #[test]
    fn local_destination() {
        let dest = parse_output_destination("/tmp/data", None).unwrap();
        assert!(matches!(dest, OutputDestination::Local(p) if p == Path::new("/tmp/data")));
    }

    #[test]
    fn malformed_s3_uri_rejected() {
        let err = parse_output_destination("s3:://bucket/prefix", None).unwrap_err();
        assert!(err.to_string().contains("malformed S3 URI"));
    }
}
