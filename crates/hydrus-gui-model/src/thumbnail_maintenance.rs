//! The thumbnail menu's manage > maintenance and viewing stats entries
//! (`ClientGUIMediaResultsPanel._RegenerateFileData`,
//! `ClientGUIMediaModalActions.DoClearFileViewingStats`): the file
//! maintenance jobs in the reference's human order with their labels and
//! descriptions (`oracle/dump_regen_jobs.py`), and what each asks.

use hydrus_core::numbers::human_int;
use hydrus_store::file_maintenance::JobType;

/// The jobs in the menu's order.
pub const HUMAN_ORDER: [JobType; 27] = [
    JobType::IntegrityPresenceTryUrlElseRemoveRecord,
    JobType::IntegrityPresenceTryUrl,
    JobType::IntegrityDataTryUrlElseRemoveRecord,
    JobType::IntegrityDataTryUrl,
    JobType::IntegrityPresenceRemoveRecord,
    JobType::IntegrityPresenceDeleteRecord,
    JobType::IntegrityDataRemoveRecord,
    JobType::IntegrityDataSilentDelete,
    JobType::IntegrityPresenceLogOnly,
    JobType::FileMetadata,
    JobType::RefitThumbnail,
    JobType::ForceThumbnail,
    JobType::Blurhash,
    JobType::PixelHash,
    JobType::PerceptualHashes,
    JobType::FileModifiedTimestamp,
    JobType::OtherHashes,
    JobType::CheckSimilarFilesMembership,
    JobType::HasTransparency,
    JobType::HasExif,
    JobType::HasXmp,
    JobType::HasIptc,
    JobType::HasHumanReadableEmbeddedMetadata,
    JobType::HasSoftwareSource,
    JobType::HasIccProfile,
    JobType::FixPermissions,
    JobType::DeleteNeighbourDupes,
];

/// A job's longer description (`regen_file_enum_to_description_lookup`).
pub fn description(job: JobType) -> &'static str {
    match job {
        JobType::IntegrityPresenceTryUrlElseRemoveRecord => {
            "THIS IS THE EASY AND QUICK ONE-SHOT WAY TO REPAIR A DATABASE WITH MISSING FILES.\n\nThis checks to see if the file is present in the file system as expected. If it is not, and it has known post/file URLs, the URLs will be automatically added to a new URL downloader.\n\nNote that if a files's URL(s) are now 404, or if they point to slightly different new duplicate files (let's say the server resized them or the CDN optimised their file cache), then hydrus will not recognise that the original file has not been 'filled in' and the broken file record will remain. In this case, you would want to run the alternate simpler 'if file is missing, remove record (leave no delete record)' job after this URL job has completely cleared and its downloader page finished, just to catch any lingering strays.\n\nMissing files with no URLs will have their internal file record in the database completely removed. This is just like a file delete except it does not leave a deletion record, so if a normal import ever sees the file again in future, it will not appear to be 'previously deleted', but completely new.\n\nAll missing files will have their hashes, tags, and URLs exported to a new folder in your database directory for later manual recovery attempts if you wish."
        }
        JobType::IntegrityPresenceTryUrl => {
            "This checks to see if the file is present in the file system as expected. If it is not, and it has known post/file URLs, the URLs will be automatically added to a new URL downloader.'\n\nNote that if a files's URL(s) are now 404, or if they point to slightly different new duplicate files (let's say the server resized them or the CDN optimised their file cache), then hydrus will not recognise that the original file has not been 'filled in' and the broken file record will remain. In this case, you would want to run the alternate simpler 'if file is missing, remove record (leave no delete record)' job after this URL job has completely cleared and its downloader page finished, just to catch any lingering strays.\n\nAll missing files will have their hashes, tags, and URLs exported to a new folder in your database directory for later manual recovery attempts if you wish."
        }
        JobType::IntegrityDataTryUrlElseRemoveRecord => {
            "This does the same check as the 'file is missing' job, and if the file is where it is expected, it ensures its file content, byte-for-byte, is as expected. This discovers hard drive damage or other external interference. This is a heavy job, so be wary. If the file is missing/incorrect _and_ has known post/file URLs, the URLs will be automatically added to a new URL downloader.\n\nMissing/Incorrect files with no URLs will have their internal file record in the database completely removed. This is just like a file delete except it does not leave a deletion record, so if a normal import ever sees the file again in future, it will not appear to be 'previously deleted', but completely new.\n\nNote that if a files's URL(s) are now 404, or if they point to slightly different new duplicate files (let's say the server resized them or the CDN optimised their file cache), then hydrus will not recognise that the original file has not been 'filled in' and the broken file record will remain. In this case, you would want to run the alternate simpler 'if file is missing, remove record (leave no delete record)' job after this URL job has completely cleared and its downloader page finished, just to catch any lingering strays.\n\nAll incorrect files will be exported to a new folder in your database directory for later manual examination if you wish.\n\nAll missing/incorrect files will also have their hashes, tags, and URLs exported to a new folder in your database directory for later manual recovery attempts if you wish."
        }
        JobType::IntegrityDataTryUrl => {
            "This does the same check as the 'file is missing' job, and if the file is where it is expected, it ensures its file content, byte-for-byte, is as expected. This discovers hard drive damage or other external interference. This is a heavy job, so be wary. If the file is missing/incorrect _and_ has known post/file URLs, the URLs will be automatically added to a new URL downloader.\n\nNote that if a files's URL(s) are now 404, or if they point to slightly different new duplicate files (let's say the server resized them or the CDN optimised their file cache), then hydrus will not recognise that the original file has not been 'filled in' and the broken file record will remain. In this case, you would want to run the alternate simpler 'if file is missing, remove record (leave no delete record)' job after this URL job has completely cleared and its downloader page finished, just to catch any lingering strays.\n\nAll incorrect files will be exported to a new folder in your database directory for later manual examination if you wish.\n\nAll missing/incorrect files will also have their hashes, tags, and URLs exported to a new folder in your database directory for later manual recovery attempts if you wish."
        }
        JobType::IntegrityPresenceRemoveRecord => {
            "This checks to see if the file is present in the file system as expected. Use this if you have lost a number of files from your file structure, do not think you can recover them, and need hydrus to re-sync with what it actually has.\n\nMissing files will have their internal file record in the database completely removed. This is just like a file delete except it does not leave a deletion record, so if a normal import ever sees the file again in future, it will not appear to be 'previously deleted', but completely new.\n\nAll missing files will have their hashes, tags, and URLs exported to a new folder in your database directory for later manual recovery attempts if you wish."
        }
        JobType::IntegrityPresenceDeleteRecord => {
            "This checks to see if the file is present in the file system as expected. Use this if you have manually deleted a number of files from your file structure, do not want to get them again, and need hydrus to re-sync with what it actually has. Another example of this situation is restoring an old backed-up database to a newer client_files structure--to catch the database up, you want to teach it that any files missing in the newer structure should be deleted, with a record.\n\nMissing files will have their internal file record processed just like a normal file delete. Normal imports that see these files again in future will consider them as 'previously deleted'.\n\nAll missing files will have their hashes, tags, and URLs exported to a new folder in your database directory for later manual recovery attempts if you wish."
        }
        JobType::IntegrityDataRemoveRecord => {
            "This does the same check as the 'file is missing' job, and if the file is where it is expected, it ensures its file content, byte-for-byte, is as expected. This discovers hard drive damage or other external interference. This is a heavy job, so be wary.\n\nMissing/Incorrect files will have their internal file record in the database completely removed. This is just like a file delete except it does not leave a deletion record, so if a normal import ever sees the file again in future, it will not appear to be 'previously deleted', but completely new.\n\nAll incorrect files will be exported to a new folder in your database directory for later manual examination if you wish.\n\nAll missing/incorrect files will also have their hashes, tags, and URLs exported to a new folder in your database directory for later manual recovery attempts if you wish."
        }
        JobType::IntegrityDataSilentDelete => {
            "If the file is where it is expected, this ensures its file content, byte-for-byte, is correct. This is a heavy job, so be wary. If the file is incorrect, it will be exported to your database directory along with its known URLs. The client's file record will not be deleted. This is useful if you have a valid backup and need to clear out invalid files from your live db so you can fill in gaps from your backup with a program like FreeFileSync."
        }
        JobType::IntegrityPresenceLogOnly => {
            "This checks to see if the file is present in the file system as expected. If it is not, it records the file's hash, tags, and URLs to your database directory, just like the other \"missing file\" jobs, but makes no other action."
        }
        JobType::FileMetadata => {
            "This regenerates file metadata like resolution and duration, or even filetype (such as mkv->webm), which may have been misparsed in a previous version."
        }
        JobType::RefitThumbnail => {
            "This looks for the existing thumbnail, and if it is not the correct resolution or is missing, will regenerate a new one for the source file."
        }
        JobType::ForceThumbnail => {
            "This forces a complete regeneration of the thumbnail from the source file."
        }
        JobType::Blurhash => {
            "This generates a very small version of the file's thumbnail that can be used as a placeholder while the thumbnail loads."
        }
        JobType::PixelHash => {
            "This generates a fast unique identifier for the pixels in a still image, which is used in duplicate pixel searches."
        }
        JobType::PerceptualHashes => {
            "This forces a regeneration of the file's similar-files 'phashes'. If phashes change and are useful, files will be queued into potential duplicate pair search. It is not useful unless you know there is missing data to repair or that the phash generation tech has changed (e.g. it fixes some file rotation), meaning for new phashes if re-generated."
        }
        JobType::FileModifiedTimestamp => {
            "This rechecks the file's modified timestamp and saves it to the database."
        }
        JobType::OtherHashes => {
            "This regenerates hydrus's store of md5, sha1, and sha512 supplementary hashes, which it can use for various external (usually website) lookups."
        }
        JobType::CheckSimilarFilesMembership => {
            "This checks to see if files should be in the search system that looks for potential duplicate pairs, and if they are falsely in or falsely out, it will remove their record or queue them up for a search as appropriate. It is useful to repair database damage."
        }
        JobType::HasTransparency => {
            "This loads the file to see if it has an alpha channel with useful data (the strictness of this test is determined in the options). Only works for images and some animations."
        }
        JobType::HasExif => {
            "This loads the file to see if it has EXIF metadata, which can be shown in the media viewer and searched with \"system:has exif\"."
        }
        JobType::HasXmp => {
            "This loads the file to see if it has XMP metadata, which can be shown in the media viewer and searched with \"system:has xmp\"."
        }
        JobType::HasIptc => {
            "This loads the file to see if it has XMP metadata, which can be shown in the media viewer and searched with \"system:has iptc\"."
        }
        JobType::HasHumanReadableEmbeddedMetadata => {
            "This loads the file to see if it has human-readable metadata outside of EXIF and such, things like a stray Artist or Title or Comment label, or something like an AI prompt, which can be shown in the media viewer and searched with \"system:has human-readable metadata\"."
        }
        JobType::HasSoftwareSource => {
            "This loads the file to see if it has software/source metadata, which can be shown in the media viewer and searched with \"system:has software/source metadata\"."
        }
        JobType::HasIccProfile => {
            "This loads the file to see if it has an ICC profile, which is generally applied in hydrus and can be searched with \"system:has icc profile\"."
        }
        JobType::FixPermissions => {
            "This ensures that files in the file system are readable and writeable. For Linux/macOS users, it specifically sets 644. If you wish to run this job on Linux/macOS, ensure you are first the file owner of all your files."
        }
        JobType::DeleteNeighbourDupes => {
            "Sometimes, a file metadata regeneration will mean a new filetype and thus a new file extension. If the existing, incorrectly named file is in use, it must be copied rather than renamed, and so there is a spare duplicate left over after the operation. This jobs cleans up the duplicate at a later time."
        }
    }
}

/// How a regeneration is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asking {
    /// Yes or no; yes runs it now.
    YesNo(String),
    /// More than 50 files: "do it now", "do it later" or "forget it".
    NowOrLater(String),
}

/// What `_RegenerateFileData` asks for `job` on `num_files` files (none for
/// no files).
pub fn regenerate_question(job: JobType, num_files: usize) -> Option<Asking> {
    if num_files == 0 {
        return None;
    }
    let n = human_int(num_files as u64);
    let mut message = match job {
        JobType::FileMetadata => format!(
            "This will reparse the {n} selected files' metadata.\n\nIf the files were imported before some more recent improvement in the parsing code (such as EXIF rotation or bad video resolution or duration or frame count calculation), this will update them."
        ),
        JobType::ForceThumbnail => {
            format!("This will force-regenerate the {n} selected files' thumbnails.")
        }
        JobType::RefitThumbnail => format!(
            "This will regenerate the {n} selected files' thumbnails, but only if they are the wrong size."
        ),
        _ => description(job).to_owned(),
    };
    Some(if num_files > 50 {
        message.push_str(&format!(
            "\n\nYou have selected {n} files, so this job may take some time. You can run it all now or schedule it to the overall file maintenance queue for later spread-out processing."
        ));
        Asking::NowOrLater(message)
    } else {
        Asking::YesNo(message)
    })
}

/// The "do it now" / "do it later" labels.
pub const NOW_OR_LATER: [&str; 2] = ["do it now", "do it later"];

/// What clearing the selection's viewing stats asks (none for no files).
pub fn clear_viewing_stats_question(num_files: usize) -> Option<String> {
    let insert = match num_files {
        0 => return None,
        1 => "this file".to_owned(),
        n => format!("these {} files", human_int(n as u64)),
    };
    Some(format!(
        "Clear the file viewing count/duration and 'last viewed time' for {insert}?"
    ))
}
