#![feature(try_blocks)]

use serde::Deserialize;
use telegraph_rs::html_to_node;
use teloxide::{prelude::*, types::ParseMode, utils::command::BotCommands};

#[derive(Deserialize)]
struct Config {
    telegram_token: String,
    telegraph_token: String,
    tsdm_api_base: String,
}

static CONFIG: once_cell::sync::Lazy<Config> = once_cell::sync::Lazy::new(|| {
    let config = std::fs::read_to_string("config.json").unwrap();
    serde_json::from_str(&config).unwrap()
});

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    log::info!("Starting command bot...");

    let bot = Bot::new(&CONFIG.telegram_token);

    Command::repl(bot, answer).await;
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "TSDM related features")]
    TSDM(String),
}

async fn answer(bot: Bot, message: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(message.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::TSDM(command) => {
            // pure number, tid
            if command.parse::<u32>().is_ok() {
                let client = reqwest::Client::new();

                let url = format!("{}/thread/{command}?buy=1", CONFIG.tsdm_api_base);
                let response = client
                    .get(url)
                    .send()
                    .await
                    .unwrap()
                    .json::<serde_json::Value>()
                    .await
                    .unwrap();
                if response["status"].as_i64().unwrap() == 0 {
                    let title = response["subject"].as_str().unwrap();
                    let post = response["postlist"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|post| post.as_object().unwrap()["message"].as_str().unwrap())
                        .collect::<Vec<_>>()
                        .join("\n<hr />\n");

                    let nodes = html_to_node(&post);
                    let nodes: serde_json::Value = serde_json::from_str(&nodes).unwrap();
                    let telegraph_response = client
                        .post("https://api.telegra.ph/createPage")
                        .json(&serde_json::json!({
                            "access_token": &CONFIG.telegraph_token,
                            "author_name": "Nijika Ijichi",
                            "title": title,
                            "content": nodes,
                        }))
                        .send()
                        .await
                        .unwrap()
                        .json::<serde_json::Value>()
                        .await
                        .unwrap();
                    let link = telegraph_response["result"].as_object().unwrap()["url"]
                        .as_str()
                        .unwrap();

                    bot.send_message(message.chat.id, link)
                        .reply_to_message_id(message.id)
                        .parse_mode(ParseMode::Html)
                        .await?;
                }
                return Ok(());
            }

            // =<page> <search keyword>
            let (page, keyword) = if command.starts_with('=') {
                let (page, keyword) = command.split_once(' ').unwrap_or(("1", &command));
                (&page[1..], keyword)
            } else {
                ("1", command.as_str())
            };

            #[derive(Deserialize)]
            struct SearchResult {
                // keywords: String,
                // page_size: usize,
                results: Vec<SearchResultItem>,
            }

            #[derive(Deserialize)]
            struct SearchResultItem {
                title: String,
                thread_id: u32,
                // timestamp: u32,

                // forum_id: u32,
                forum_name: String,
                // author_id: u32,
                // author_name: String,
            }

            let response: anyhow::Result<SearchResult> = try {
                let params = [("query", keyword), ("page", page)];
                let url = reqwest::Url::parse_with_params(
                    &format!("{}/search", CONFIG.tsdm_api_base),
                    &params,
                )?;
                reqwest::get(url).await?.json::<SearchResult>().await?
            };

            match response {
                Ok(result) => {
                    let result = result
                        .results
                        .iter()
                        .enumerate()
                        .map(|(index, item)| {
                            let index = index + 1;
                            let thread_id = item.thread_id;
                            let forum_name = &item.forum_name;
                            let title = &item.title;
                            format!("[{index:02}] <code>{thread_id}</code>({forum_name}) {title}")
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    bot.send_message(message.chat.id, result)
                        .reply_to_message_id(message.id)
                        .parse_mode(ParseMode::Html)
                        .await?;
                }
                Err(e) => {
                    bot.send_message(message.chat.id, format!("Failed to search: {}", e))
                        .reply_to_message_id(message.id)
                        .await?;
                }
            };
        }
    };

    Ok(())
}
