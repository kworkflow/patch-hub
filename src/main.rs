use lore_peek::lore_session::LoreSession;
use lore_peek::patch::Patch;
use lore_peek::lore_api_client::FailedFeedRequest;
use std::env;
use std::io::{self, Write};
use std::process::exit;

fn main() {
    let target_list: String;
    let page_size: u32;

    (target_list, page_size) = parse_args();
    main_loop(target_list, page_size);
}

fn parse_args() -> (String, u32) {
    let args: Vec<_> = env::args().collect();
    let target_list: String;
    let page_size: u32;

    if args.len() != 3 {
        panic!("[errno::EINVAL]\n*\tWrong number of arguments\n*\tUsage: cargo run <target_list> <page_size>");
    }

    target_list = String::from(&args[1]);
    page_size = args[2].parse::<u32>().unwrap();

    if page_size == 0 {
        panic!("[errno::EINVAL]\n*\tpage_size should be non-zero positives")
    }

    (target_list, page_size)
}

fn main_loop(target_list: String, page_size: u32) {
    let mut lore_session: LoreSession;
    let mut page_number: u32 = 1;

    lore_session = LoreSession::new(target_list.clone());

    loop {
        if let Err(failed_feed_request) = lore_session.process_n_representative_patches(page_size * page_number) {
            match failed_feed_request {
                FailedFeedRequest::UnknownError(error) => panic!("[FailedFeedRequest::UnknownError]\n*\tFailed to request feed\n*\t{error:#?}"),
                FailedFeedRequest::StatusNotOk(feed_response) => panic!("[FailedFeedRequest::StatusNotOk]\n*\tRequest returned with non-OK status\n*\t{feed_response:#?}"),
                FailedFeedRequest::EndOfFeed => panic!("[FailedFeedRequest::EndOfFeed]\n*\tReached end of feed"),
            }
        };

        print_patch_feed_page(&lore_session, &target_list, page_size, page_number);

        match collect_user_command() {
            'N' | 'n' => page_number += 1,
            'P' | 'p' => if page_number != 1 { page_number -= 1 },
            'Q' | 'q' => exit(0),
            _ => panic!("[errno::EINVAL]\n*\tInvalid command. It shouldn't get to here...")
        }
    }
}

fn print_patch_feed_page(lore_session: &LoreSession, target_list: &String, page_size: u32, page_number: u32) {
    let patch_feed_page: Vec<&Patch>;
    let mut index: u32;

    println!("======================= {target_list} pg. {page_number} =======================");
    patch_feed_page = lore_session.get_patch_feed_page(page_size, page_number);
    index = page_size * (page_number - 1);
    for patch in patch_feed_page {
        println!(
            "{:03}. V{} | #{:02} | {} | {}",
            index, patch.get_version(), patch.get_total_in_series(), patch.get_title(), patch.get_author().name
        );

        index += 1;
    }
    println!("======================= {target_list} pg. {page_number} =======================\n");
}

fn collect_user_command() -> char {
    let mut input: String = String::new();
    let command_code: char;

    loop {
        input.clear();

        print!("Enter a command [ (n)ext | (p)revious | (q)uit]: ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut input).unwrap();
        if input.len() == 2 {
            if let Some(char) = input.trim().chars().next() {
                match char {
                    'N' | 'n' | 'P' | 'p' | 'Q' | 'q' => {
                        command_code = char;
                        break;
                    },
                    _ => (),
                }
            }
        };

        println!("Invalid input!");
    }

    command_code
}
