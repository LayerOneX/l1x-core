use tokio::fs::File;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::path::Path;

use log::{error, info, debug};
use tokio::sync::RwLock;
use tokio::time;
use block::block_state::BlockState;
use db::db::Database;
use node_crate::sync::{CHAIN_STATE, DB_DUMP, DB_DUMP_SIGNATURE};
use primitives::Address;
use sha2::{Sha256, Digest};
use secp256k1::Secp256k1;
use secp256k1::SecretKey;
use secp256k1::Message;

use tokio::io::AsyncReadExt;
use anyhow::{anyhow, Error};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::{Client as S3Client, Config};


use tokio::io::AsyncWriteExt;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::types::{CompletedPart, CompletedMultipartUpload };
use aws_smithy_http::byte_stream::Length;
use tokio::time::sleep;
use std::io::ErrorKind;

pub async fn upload_db_dump_to_s3(time_interval: u64, cluster_address: Address, database_url: String, node_private_key: String, file_mutex: Arc<RwLock<()>>) {
    let duration = Duration::from_secs(time_interval);
    // Create a timer that fires at specified intervals
    let mut interval = time::interval(duration);
    loop {
        // Wait until the next interval
        interval.tick().await;
        info!("Creating db dump...");
        let guard = file_mutex.write().await;
        let dump_creation_start_time = Instant::now();
        match Command::new("pg_dump")
            .args(&["-Fc", "--file", DB_DUMP, &database_url])
            .output() {
            Ok(output) => {
                if output.status.success() {
                    info!("Database dump created successfully");
                    info!(
                        "⌛️ Database dump created in: {:?} seconds",
                        dump_creation_start_time.elapsed().as_secs()
                    );
                    match create_and_sign_db_dump_hash(node_private_key.clone()).await {
                        Ok(_) => {
                            match create_chain_state_file(cluster_address).await {
                                Ok(_) => {
                                    info!("Uploading to S3");
                                    if let Err(error) = setup_and_upload_files_to_s3().await {
                                        error!("{:?}", error);
                                    }
                                },
                                Err(error) => error!("{:?}", error),
                            }
                        },
                        Err(error) => error!("{:?}", error),
                    }
                } else {
                    log::error!("Error creating database dump: {:?}", String::from_utf8_lossy(&output.stderr));
                }
            },
            Err(error) => log::error!("Failed to execute pg_dump command: {:?}", error)
        }
        drop(guard);
    }
}

async fn create_and_sign_db_dump_hash(node_private_key: String) -> Result<(), Error> {
    // Create a Secp256k1 context
    let secp = Secp256k1::new();

    // Create a SecretKey from the byte array
    let secret_key = SecretKey::from_slice(
        &hex::decode(node_private_key.as_str()).map_err(|e| {
            anyhow!("Error decoding node_private_key: {:?}", e)
        })?,
    ).map_err(|e| {
        anyhow!("Failed to parse provided private_key: {:?}", e)
    })?;

    // Open the file
    let mut file = File::open(DB_DUMP).await.map_err(|e| {
        anyhow!("Failed to open db_dump: {:?}", e)
    })?;

    // Initialize SHA-256 hasher
    let mut hasher = Sha256::new();

    // Define the chunk size (e.g., 1 MB)
    let chunk_size = 1024 * 1024;
    let max_retries: usize = 3;
    let mut chunk = vec![0; chunk_size];
    let mut retries = 0;
    // Read and hash the file in chunks
    loop {
        if retries >= max_retries {
            return Err(anyhow!("Maximum retries exceeded"));
        }
        let result = file.read(&mut chunk).await;
        match result {
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    break; // End of file
                }
                hasher.update(&chunk[..bytes_read]);
                retries = 0; // Reset retries on successful read
            }
            Err(err) => match err.kind() {
                ErrorKind::Interrupted => {
                    debug!("Read interrupted, retrying...");
                    retries += 1;
                    continue; // Retry the read operation
                }
                _ => {
                    debug!("Error reading db_dump: {:?}", err);
                    return Err(anyhow!("Error reading db_dump: {:?}", err));
                }
            },
        }
    }

    // Finalize the hash
    let hash_result = hasher.finalize();
    let message = Message::from_digest_slice(&hash_result).map_err(|e| {
        anyhow!("Failed to create Message: {:?}", e)
    })?;

    // Sign the message using the SecretKey
    let signature = secp.sign_ecdsa(&message, &secret_key);
    // Print the signature
    println!("Hash: {:?}", message);
    println!("Signature: {:?}", signature);
    println!("SHA-256 hash of {:?}:", DB_DUMP);
    // Serialize the signature to a byte array
    let serialized_signature = signature.serialize_compact();
    // Store the serialized signature to a file
    let mut signature_file = File::create(DB_DUMP_SIGNATURE).await.map_err(|e| {
        anyhow!("Failed to create signature file: {:?}", e)
    })?;
    signature_file.write_all(&serialized_signature).await.map_err(|e| {
        anyhow!("Failed to write signature to file: {:?}", e)
    })?;
    //println!("Signature has been written to {}", signature_file_path);
    Ok(())
}

async fn setup_and_upload_files_to_s3() -> Result<(), Error> {
    // Provide your access key ID, secret access key, region, and bucket name
    let access_key = std::env::var("ACCESS_KEY")?;
    let secret_key = std::env::var("SECRET_KEY")?;
    let region = std::env::var("REGION")?;
    let bucket_name = std::env::var("BUCKET_NAME")?;
    let max_retries = 3;

    // Initialize AWS configuration
    let credentials = Credentials::new(access_key, secret_key, None, None, "");
    let config = Config::builder()
        .region(Region::new(region))
        .credentials_provider(credentials)
        .build();
    let client = S3Client::from_conf(config);
    // Specify the S3 bucket name and file paths
    // Upload the files to S3
    let mut attempt = 0;
    let upload_start_time = Instant::now();
    while attempt < max_retries {
        match upload_files_to_s3(&client, &bucket_name).await {
            Ok(_) => {
                info!("Files uploaded to S3 successfully");
                break;
            },
            Err(error) => {
                attempt += 1;
                error!("Failed to upload files after {} retries: {:?}", attempt, error);
                // Exponential backoff before retrying
                let seconds = 2_u64.pow(attempt);
                let backoff_duration = Duration::from_secs(seconds);
                info!("Waiting for {:?} seconds", seconds);
                sleep(backoff_duration).await;
            }
        }
    }
    if attempt >= max_retries {
        Err(anyhow!("Failed to upload file"))
    } else {
        info!(
            "⌛️ Files uploaded in : {:?} seconds",
            upload_start_time.elapsed().as_secs()
        );
        Ok(())
    }
}

async fn upload_files_to_s3(
    client: &S3Client,
    bucket_name: &str,
) -> Result<(), Error> {
    //In bytes, minimum chunk size of 5MB. Increase CHUNK_SIZE to send larger chunks.
    let chunk_size: u64 = 1024 * 1024 * 5;
    let max_chunk: u64 = 10000;
    let max_retries = 3;
    let mut retry_count = 0;
    let file_paths = vec![
        DB_DUMP, DB_DUMP_SIGNATURE, CHAIN_STATE
    ];

    for file_path in file_paths {
        let multipart_upload_res = client.create_multipart_upload()
            .bucket(bucket_name)
            .key(file_path)
            .send()
            .await
            .map_err(|e| anyhow!("Unable to create CreateMultipartUploadOutput: {:?}", e))?;
        let upload_id = multipart_upload_res.upload_id().ok_or_else(|| anyhow!("Upload ID is None"))?;

        let path = Path::new(file_path);
        let file_size = tokio::fs::metadata(path).await.map_err(|e| anyhow!("File not exist: {:?}", e))?.len();
        let mut chunk_count = (file_size / chunk_size) + 1;
        let mut size_of_last_chunk = file_size % chunk_size;
        if size_of_last_chunk == 0 {
            size_of_last_chunk = chunk_size;
            chunk_count -= 1;
        }

        if file_size == 0 {
            return Err(anyhow!("Bad file size."));
        }
        if chunk_count > max_chunk {
            return Err(anyhow!("Too many chunks! Try increasing your chunk size."));
        }
        let mut upload_parts: Vec<CompletedPart> = Vec::new();
        for chunk_index in 0..chunk_count {
            let this_chunk = if chunk_count - 1 == chunk_index {
                size_of_last_chunk
            } else {
                chunk_size
            };
            let mut attempt = 0;
            while attempt < max_retries {
                let stream = ByteStream::read_from()
                    .path(path)
                    .offset(chunk_index * chunk_size)
                    .length(Length::Exact(this_chunk))
                    .build()
                    .await
                    .map_err(|e| anyhow!("Byte steam read error: {:?}", e))?;
                let part_number = (chunk_index as i32) + 1;
                match client.upload_part()
                    .key(file_path)
                    .bucket(bucket_name)
                    .upload_id(upload_id)
                    .body(stream)
                    .part_number(part_number)
                    .send()
                    .await {
                    Ok(upload_part_res) => {
                        upload_parts.push(
                            CompletedPart::builder()
                                .e_tag(upload_part_res.e_tag.unwrap_or_default())
                                .part_number(part_number)
                                .build(),
                        );
                        break;
                    },
                    Err(err) => {
                        attempt += 1;
                        error!("Failed to upload chunk {} after {} retries: {:?}", part_number, attempt, err);
                        // Exponential backoff before retrying
                        let seconds = 2_u64.pow(attempt);
                        let backoff_duration = Duration::from_secs(seconds);
                        info!("Waiting for {:?}", seconds);
                        sleep(backoff_duration).await;
                    }
                }
            }
            if attempt >= max_retries {
                // Clean up partially uploaded parts
                info!("Cleaning up partially uploaded parts...");
                let _abort_multipart_upload_res = client.abort_multipart_upload()
                    .bucket(bucket_name)
                    .key(file_path)
                    .upload_id(upload_id)
                    .send()
                    .await
                    .map_err(|e| anyhow!("Failed to abort multipart upload: {:?}", e))?;
                return Err(anyhow!("Failed to upload chunk after {} retries.", max_retries));
            }
        }
        let completed_multipart_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(upload_parts))
            .build();

        let _complete_multipart_upload_res = client.complete_multipart_upload()
            .bucket(bucket_name)
            .key(file_path)
            .multipart_upload(completed_multipart_upload)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to complete multipart upload: {:?}", e))?;
    }
    Ok(())
}

async fn create_chain_state_file(cluster_address: Address,) -> Result<(), Error> {
    let mut connection = Database::get_pool_connection().await?;
    let block_state = BlockState::new(&connection).await?;
    let chain_state = block_state
        .load_chain_state(cluster_address)
        .await
        .expect("unable to load_chain_state");
    info!("Creating chain_state.json file");
    // Serialize the struct to a JSON string
    let json = serde_json::to_string(&chain_state).map_err(|e| {
        anyhow!("Failed to serialize chain state: {:?}", e)
    })?;
    let mut chain_state_file = File::create(CHAIN_STATE).await.map_err(|e| {
        anyhow!("Failed to create signature file: {:?}", e)
    })?;
    chain_state_file.write_all(json.as_bytes()).await.map_err(|e| {
        anyhow!("Failed to write signature to file: {:?}", e)
    })?;
    Ok(())
}