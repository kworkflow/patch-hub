use crate::lore_api_client::{
    AvailableListsRequest, FailedAvailableListsRequest, FailedFeedRequest, FailedPatchHTMLRequest,
    PatchFeedRequest, PatchHTMLRequest,
};
use crate::mailing_list::MailingList;
use crate::patch::{Patch, PatchFeed, PatchRegex};
use regex::Regex;
use serde_xml_rs::from_str;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::mem::swap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::{
    fs::{self, File},
    io,
};

#[cfg(test)]
mod tests;

const LORE_PAGE_SIZE: usize = 200;

pub struct LoreSession {
    representative_patches_ids: Vec<String>,
    processed_patches_map: HashMap<String, Patch>,
    patch_regex: PatchRegex,
    target_list: String,
    min_index: usize,
}

impl LoreSession {
    pub fn new(target_list: String) -> LoreSession {
        LoreSession {
            target_list,
            representative_patches_ids: Vec::new(),
            processed_patches_map: HashMap::new(),
            patch_regex: PatchRegex::new(),
            min_index: 0,
        }
    }

    pub fn get_representative_patches_ids(&self) -> &Vec<String> {
        &self.representative_patches_ids
    }

    pub fn get_processed_patch(&self, message_id: &str) -> Option<&Patch> {
        self.processed_patches_map.get(message_id)
    }

    pub fn process_n_representative_patches<T: PatchFeedRequest>(
        &mut self,
        lore_api_client: &T,
        n: usize,
    ) -> Result<(), FailedFeedRequest> {
        let mut patch_feed: PatchFeed;
        let mut processed_patches_ids: Vec<String>;

        while self.representative_patches_ids.len() < n {
            match lore_api_client.request_patch_feed(&self.target_list, self.min_index) {
                Ok(feed_response_body) => patch_feed = from_str(&feed_response_body).unwrap(),
                Err(failed_feed_request) => return Err(failed_feed_request),
            }

            processed_patches_ids = self.process_patches(patch_feed);
            self.update_representative_patches(processed_patches_ids);

            self.min_index += LORE_PAGE_SIZE;
        }

        Ok(())
    }

    fn process_patches(&mut self, patch_feed: PatchFeed) -> Vec<String> {
        let mut processed_patches_ids: Vec<String> = Vec::new();

        for mut patch in patch_feed.get_patches() {
            patch.update_patch_metadata(&self.patch_regex);

            if !self
                .processed_patches_map
                .contains_key(&patch.get_message_id().href)
            {
                processed_patches_ids.push(patch.get_message_id().href.clone());
                self.processed_patches_map
                    .insert(patch.get_message_id().href.clone(), patch);
            }
        }

        processed_patches_ids
    }

    fn update_representative_patches(&mut self, processed_patches_ids: Vec<String>) {
        let mut patch: &Patch;
        let mut patch_number_in_series: usize;

        for message_id in processed_patches_ids {
            patch = self.processed_patches_map.get(&message_id).unwrap();
            patch_number_in_series = patch.get_number_in_series();

            if patch_number_in_series > 1 {
                continue;
            }

            if patch_number_in_series == 1 {
                if let Some(in_reply_to) = &patch.get_in_reply_to() {
                    if let Some(patch_in_reply_to) =
                        self.processed_patches_map.get(&in_reply_to.href)
                    {
                        if (patch_in_reply_to.get_number_in_series() == 0)
                            && (patch.get_version() == patch_in_reply_to.get_version())
                        {
                            continue;
                        };
                    };
                };
            }

            self.representative_patches_ids
                .push(patch.get_message_id().href.clone());
        }
    }

    pub fn get_patch_feed_page(&self, page_size: usize, page_number: usize) -> Option<Vec<&Patch>> {
        let mut patch_feed_page: Vec<&Patch> = Vec::new();
        let representative_patches_ids_max_index: usize = self.representative_patches_ids.len() - 1;
        let lower_end: usize = page_size * (page_number - 1);
        let mut upper_end: usize = page_size * page_number;

        if representative_patches_ids_max_index <= lower_end {
            return None;
        }

        if representative_patches_ids_max_index < upper_end - 1 {
            upper_end = representative_patches_ids_max_index + 1;
        }

        for i in lower_end..upper_end {
            patch_feed_page.push(
                self.processed_patches_map
                    .get(&self.representative_patches_ids[i])
                    .unwrap(),
            )
        }

        Some(patch_feed_page)
    }
}

pub fn download_patchset(output_dir: &str, patch: &Patch) -> io::Result<String> {
    let message_id: &str = &patch.get_message_id().href;
    let mbox_name: String = extract_mbox_name_from_message_id(message_id);

    if !Path::new(output_dir).exists() {
        fs::create_dir_all(output_dir)?;
    }

    let filepath: String = format!("{output_dir}/{mbox_name}");
    if !Path::new(&filepath).exists() {
        Command::new("b4")
            .arg("--quiet")
            .arg("am")
            .arg("--use-version")
            .arg(format!("{}", patch.get_version()))
            .arg(message_id)
            .arg("--outdir")
            .arg(output_dir)
            .arg("--mbox-name")
            .arg(&mbox_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }

    Ok(filepath)
}

fn extract_mbox_name_from_message_id(message_id: &str) -> String {
    let mut mbox_name: String = message_id
        .replace(r#"http://lore.kernel.org/"#, "")
        .replace(r#"https://lore.kernel.org/"#, "")
        .replace('/', ".");

    if !mbox_name.ends_with('.') {
        mbox_name.push('.');
    }
    mbox_name.push_str("mbx");

    mbox_name
}

pub fn split_patchset(patchset_path_str: &str) -> Result<Vec<String>, String> {
    let mut patches: Vec<String> = Vec::new();
    let patchset_path: &Path = Path::new(patchset_path_str);
    let cover_letter_path_str: String = patchset_path_str.replace(".mbx", ".cover");
    let cover_letter_path: &Path = Path::new(&cover_letter_path_str);

    if !patchset_path.exists() {
        return Err(format!("{}: Path doesn't exist", patchset_path.display()));
    } else if !patchset_path.is_file() {
        return Err(format!("{}: Not a file", patchset_path.display()));
    }

    if cover_letter_path.exists() && cover_letter_path.is_file() {
        extract_patches(cover_letter_path, &mut patches);
    }

    extract_patches(patchset_path, &mut patches);

    Ok(patches)
}

fn extract_patches(mbox_path: &Path, patches: &mut Vec<String>) {
    let mut current_patch: String = String::new();
    let mut is_reading_patch: bool = false;
    let mut is_last_line: bool = false;

    let mbox_reader: BufReader<fs::File> = io::BufReader::new(fs::File::open(mbox_path).unwrap());

    for line in mbox_reader.lines() {
        let line = line.unwrap();

        if line.starts_with("Subject: ") {
            is_reading_patch = true;
        } else if is_reading_patch && line.trim_end().eq("--") {
            is_last_line = true;
        } else if is_last_line {
            current_patch.push_str(&line);
            current_patch.push('\n');

            let mut patch_to_add = String::new();
            swap(&mut patch_to_add, &mut current_patch);
            patches.push(patch_to_add);

            is_reading_patch = false;
            is_last_line = false;
        } else if is_reading_patch && line.trim_end().eq("From git@z Thu Jan  1 00:00:00 1970") {
            let mut patch_to_add = String::new();
            swap(&mut patch_to_add, &mut current_patch);
            patches.push(patch_to_add);

            is_reading_patch = false;
        }

        if is_reading_patch {
            current_patch.push_str(&line);
            current_patch.push('\n');
        }
    }

    if !current_patch.is_empty() {
        patches.push(current_patch);
    }
}

pub fn save_bookmarked_patchsets(
    bookmarked_patchsets: &Vec<Patch>,
    filepath: &str,
) -> io::Result<()> {
    if let Some(parent) = Path::new(filepath).parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_filename = format!("{}.tmp", filepath);
    {
        let tmp_file = File::create(&tmp_filename)?;
        serde_json::to_writer(tmp_file, &bookmarked_patchsets)?;
    }
    fs::rename(tmp_filename, filepath)?;
    Ok(())
}

pub fn load_bookmarked_patchsets(filepath: &str) -> io::Result<Vec<Patch>> {
    let bookmarked_patchsets_file = File::open(filepath)?;
    let bookmarked_patchesets = serde_json::from_reader(bookmarked_patchsets_file)?;
    Ok(bookmarked_patchesets)
}

pub fn fetch_available_lists<T>(
    lore_api_client: &T,
) -> Result<Vec<MailingList>, FailedAvailableListsRequest>
where
    T: AvailableListsRequest,
{
    let mut available_lists: Vec<MailingList> = Vec::new();
    let mut min_index = 0;

    loop {
        let available_lists_str: String = match lore_api_client.request_available_lists(min_index) {
            Ok(value) => value,
            Err(failed_available_lists_request) => return Err(failed_available_lists_request),
        };

        let mut tmp_available_lists = process_available_lists(available_lists_str);

        if tmp_available_lists.is_empty() {
            break;
        }

        available_lists.append(&mut tmp_available_lists);

        min_index += LORE_PAGE_SIZE;
    }

    available_lists.sort();

    Ok(available_lists)
}

fn process_available_lists(available_lists_str: String) -> Vec<MailingList> {
    let re_pre_block: Regex = Regex::new(r#"(?s)<pre>(.*?)</pre>"#).unwrap();
    let re_list_name = Regex::new(r#"(?s)<a\s*href=".*?">(.*?)</a>"#).unwrap();
    let re_list_description = Regex::new(r#"(?s)</a>\s*(.*?)\s*\*"#).unwrap();
    let mut list_names: Vec<&str> = Vec::new();
    let mut list_descriptions: Vec<&str> = Vec::new();
    let mut available_lists: Vec<MailingList> = Vec::new();

    let pre_blocks: Vec<&str> = re_pre_block
        .captures_iter(&available_lists_str)
        .map(|cap| cap.get(1).unwrap().as_str())
        .collect();

    for capture in re_list_name.captures_iter(pre_blocks[2]) {
        let name = capture.get(1).unwrap().as_str().trim();
        list_names.push(name);
    }

    for capture in re_list_description.captures_iter(pre_blocks[2]) {
        let description = capture.get(1).unwrap().as_str().trim();
        list_descriptions.push(description);
    }

    let pairs: Vec<(&str, &str)> = list_names.into_iter().zip(list_descriptions).collect();

    for (name, description) in pairs {
        if name == "all" {
            continue;
        }
        available_lists.push(MailingList::new(name, description));
    }

    available_lists
}

pub fn save_available_lists(available_lists: &Vec<MailingList>, filepath: &str) -> io::Result<()> {
    if let Some(parent) = Path::new(filepath).parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_filename = format!("{}.tmp", filepath);
    {
        let tmp_file = File::create(&tmp_filename)?;
        serde_json::to_writer(tmp_file, &available_lists)?;
    }
    fs::rename(tmp_filename, filepath)?;
    Ok(())
}

pub fn load_available_lists(filepath: &str) -> io::Result<Vec<MailingList>> {
    let available_lists_file = File::open(filepath)?;
    let available_lists = serde_json::from_reader(available_lists_file)?;
    Ok(available_lists)
}

pub fn prepare_reply_patchset_with_reviewed_by<T>(
    lore_api_client: &T,
    tmp_dir: &Path,
    target_list: &str,
    patches: &[String],
    git_signature: &str,
) -> Result<Vec<Command>, FailedPatchHTMLRequest>
where
    T: PatchHTMLRequest,
{
    let mut git_reply_commands: Vec<Command> = Vec::new();
    let re_message_id = Regex::new(r#"(?m)^Message-Id: <(.*?)>"#).unwrap();

    for patch in patches.iter() {
        let message_id = re_message_id
            .captures(patch)
            .unwrap()
            .get(1)
            .unwrap()
            .as_str();

        let reply_path = tmp_dir.join(format!("{message_id}-reply.mbx"));
        let mut reply = generate_patch_reply_template(patch);
        reply.push_str(&format!("\nReviewed-by: {git_signature}\n"));
        fs::write(&reply_path, &reply).unwrap();

        let patch_html = match lore_api_client.request_patch_html(target_list, message_id) {
            Ok(response_payload) => response_payload,
            Err(failed_patch_html_request) => return Err(failed_patch_html_request),
        };

        let mut git_reply_command = extract_git_reply_command(&patch_html);
        git_reply_command.arg(format!("{}", reply_path.display()));

        git_reply_commands.push(git_reply_command);
    }

    Ok(git_reply_commands)
}

fn generate_patch_reply_template(patch_contents: &str) -> String {
    let mut reply_template = String::new();
    let mut patch_lines_iterator = patch_contents.lines();

    // Process the headers
    for line in patch_lines_iterator.by_ref() {
        let mut line_to_push = String::new();

        if line.starts_with("Subject: ") {
            line_to_push = line.replace("Subject: ", "Subject: Re: ") + "\n";
        } else if line.starts_with("From: ")
            || line.starts_with("Date: ")
            || line.starts_with("Message-Id: ")
        {
            continue;
        } else if !line.trim().is_empty() {
            line_to_push = format!("{}\n", line);
        } else if line.trim().is_empty() && !reply_template.is_empty() {
            reply_template.push('\n');
            break;
        }

        reply_template.push_str(&line_to_push);
    }

    // After processing headers, just quote-reply remaining lines
    for line in patch_lines_iterator {
        reply_template.push_str(&format!("> {}\n", line));
    }

    reply_template
}

fn extract_git_reply_command(patch_html: &str) -> Command {
    let mut git_reply_command = Command::new("git");
    git_reply_command.arg("send-email");
    // To avoid any chance of sending the reply while validating, add `--dry-run`
    git_reply_command.arg("--dry-run");
    git_reply_command.arg("--suppress-cc=all");

    let re_full_git_command =
        Regex::new(r#"(?s)git-send-email\(1\):(.*?)/path/to/YOUR_REPLY"#).unwrap();
    let re_long_options = Regex::new(r"--[^\s=]+=[^\s]+").unwrap();

    if let Some(capture) = re_full_git_command.captures(patch_html) {
        if let Some(full_git_command_match) = capture.get(1) {
            let full_git_command = full_git_command_match.as_str();

            for long_option_match in re_long_options.find_iter(full_git_command) {
                git_reply_command.arg(long_option_match.as_str());
            }
        }
    }

    git_reply_command
}

pub fn get_git_signature(git_repo_path: &str) -> (String, String) {
    let mut git_user_name_command = Command::new("git");
    if !git_repo_path.is_empty() {
        git_user_name_command.arg("-C").arg(git_repo_path);
    }
    let git_user_name_output = git_user_name_command
        .arg("config")
        .arg("user.name")
        .output()
        .unwrap();
    let git_user_name = std::str::from_utf8(&git_user_name_output.stdout)
        .unwrap()
        .trim();

    let mut git_user_email_command = Command::new("git");
    if !git_repo_path.is_empty() {
        git_user_email_command.arg("-C").arg(git_repo_path);
    }
    let git_user_email_output = git_user_email_command
        .arg("config")
        .arg("user.email")
        .output()
        .unwrap();
    let git_user_email = std::str::from_utf8(&git_user_email_output.stdout)
        .unwrap()
        .trim();

    (git_user_name.to_owned(), git_user_email.to_owned())
}

pub fn save_reviewed_patchsets(
    reviewed_patchsets: &HashMap<String, Vec<usize>>,
    filepath: &str,
) -> io::Result<()> {
    if let Some(parent) = Path::new(filepath).parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_filename = format!("{}.tmp", filepath);
    {
        let tmp_file = File::create(&tmp_filename)?;
        serde_json::to_writer(tmp_file, &reviewed_patchsets)?;
    }
    fs::rename(tmp_filename, filepath)?;
    Ok(())
}

pub fn load_reviewed_patchsets(filepath: &str) -> io::Result<HashMap<String, Vec<usize>>> {
    let reviewed_patchsets_file = File::open(filepath)?;
    let reviewed_patchsets = serde_json::from_reader(reviewed_patchsets_file)?;
    Ok(reviewed_patchsets)
}
