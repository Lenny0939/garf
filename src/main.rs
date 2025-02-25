use dotenv::dotenv;
use futures::stream;
use poise::{
    CreateReply,
    serenity_prelude::{
        self as serenity, CreateEmbed, UserId,
        futures::{self, Stream, StreamExt},
    },
};
use sqlx::{Row, SqlitePool};

struct Data {} // User data, which is stored and accessible in all command invocations

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

async fn autocomplete_focus<'a>(
    _ctx: Context<'_>,
    focus: &'a str,
) -> impl Stream<Item = String> + 'a {
    futures::stream::iter(&[
        "sfb",
        "sfs",
        "alt",
        "inroll",
        "outroll",
        "onehands",
        "redirects",
    ])
    .filter(move |name| futures::future::ready(name.to_lowercase().contains(&focus.to_lowercase())))
    .map(|name| name.to_string())
}
async fn autocomplete_layout<'a>(
    _ctx: Context<'_>,
    layout: &'a str,
) -> impl Stream<Item = String> + 'a {
    // Connect to DB and fetch layout names
    let db_path = std::env::var("GARFDB_PATH").unwrap_or("/var/lib/garf/scores.db".into());
    let pool = SqlitePool::connect(&format!("sqlite:{}", db_path))
        .await
        .unwrap();
    let rows = sqlx::query("SELECT Name FROM Layout")
        .fetch_all(&pool)
        .await
        .unwrap();

    let layouts_vec: Vec<String> = rows.into_iter().map(|row| row.get("Name")).collect();

    stream::iter(layouts_vec).filter_map(move |name| {
        let layout_lower = layout.to_lowercase();
        async move {
            if name.to_lowercase().contains(&layout_lower) {
                Some(name)
            } else {
                None
            }
        }
    })
}

#[poise::command(slash_command, prefix_command)]
async fn upload_layout(
    ctx: Context<'_>,
    #[description = "Creator of the layout"] creator: UserId,
    #[description = "Name of the layout"] name: String,
    #[description = "Magic flag"] magic: bool,
    #[description = "Thumb alpha flag"] thumb_alpha: bool,
    #[description = "Focus type"]
    #[autocomplete = "autocomplete_focus"]
    focus: String,
) -> Result<(), Error> {
    let db_path = std::env::var("GARFDB_PATH").unwrap_or("/var/lib/garf/scores.db".into());
    let pool = SqlitePool::connect(&format!("sqlite:{}", db_path)).await?;

    sqlx::query(
        r#"
        INSERT INTO layout (Name, Creator, Magic, ThumbAlpha, Focus)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(name.to_lowercase())
    .bind(creator.to_string())
    .bind(magic)
    .bind(thumb_alpha)
    .bind(focus)
    .execute(&pool)
    .await?;

    ctx.say("Layout uploaded successfully!").await?;
    Ok(())
}
#[poise::command(slash_command, prefix_command)]
async fn leaderboard(
    ctx: Context<'_>,
    #[description = "Filter by user"] user_filter: Option<String>,
    #[description = "Filter by layout"]
    #[autocomplete = "autocomplete_layout"]
    layout_filter: Option<String>,
    #[description = "Filter by magic"] magic_filter: Option<bool>,
    #[description = "Filter by thumb alpha"] thumb_alpha_filter: Option<bool>,
    #[description = "Filter by focus"]
    #[autocomplete = "autocomplete_focus"]
    focus_filter: Option<String>,
    #[description = "Filter by creator"] creator_filter: Option<String>,
    #[description = "Page to view"] page: Option<u8>,
) -> Result<(), Error> {
    // Defer the response to indicate the bot is processing
    ctx.defer().await?;

    let db_path = std::env::var("GARFDB_PATH").unwrap_or("/var/lib/garf/scores.db".into());
    let pool = SqlitePool::connect(&format!("sqlite:{}", db_path)).await?;

    let page_unwrapped = match page {
        Some(ref page) => page - 1,
        None => 0,
    };
    // Extract the raw user ID from the creator_filter string
    let creator_id = match creator_filter {
        Some(ref creator) => {
            if creator.starts_with("<@") && creator.ends_with('>') {
                Some(&creator[2..creator.len() - 1])
            } else {
                creator_filter.as_deref()
            }
        }
        None => creator_filter.as_deref(),
    };

    // Extract the raw user ID from the user_filter string
    let user_id = match user_filter {
        Some(ref user) => {
            if user.starts_with("<@") && user.ends_with('>') {
                Some(&user[2..user.len() - 1])
            } else {
                user_filter.as_deref()
            }
        }
        None => user_filter.as_deref(),
    };

    let layout_lowercase: Option<String> = match layout_filter {
        Some(ref layout) => Some(layout.to_lowercase()),
        None => None,
    };

    // Execute the query
    let rows = sqlx::query(
        r#"
        SELECT 
            User,
            Speed,
            layout.Name AS Layout,
            Magic, 
            ThumbAlpha, 
            Focus, 
            Creator 
        FROM 
            score
            INNER JOIN layout USING (LayoutId)
        WHERE User = COALESCE(?1, User)
            AND Layout = COALESCE(?2, Layout)
            AND Magic = COALESCE(?3, Magic)
            AND ThumbAlpha = COALESCE(?4, ThumbAlpha)
            AND Focus = COALESCE(?5, Focus)
            AND Creator = COALESCE(?6, Creator)
        ORDER BY Speed DESC
        "#,
    )
    .bind(user_id)
    .bind(layout_lowercase)
    .bind(magic_filter)
    .bind(thumb_alpha_filter)
    .bind(focus_filter)
    .bind(creator_id)
    .fetch_all(&pool)
    .await?;

    // Build the message
    let mut message = String::new();
    let mut i = 1;
    let mut strings: Vec<String> = vec![];
    for row in rows {
        //if i > page_unwrapped * 10 && i <= page_unwrapped * 10 + 10 {
            //message.push_str(&format!(
            if strings.len() < i {
                strings.push(String::default());
            }
            strings[i/10].push_str(&format!(
                "#{} **{} WPM**: <@{}> on {}\n",
                i,
                &row.get::<i64, _>("Speed"),
                row.get::<String, _>("User"),
                &row.get::<String, _>("Layout")
            ));
        //}
        i += 1;
    }

    // Create the embed
    /* let embed = CreateEmbed::new()
        .title("Leaderboard")
        .field("Scores", message, false);
    ctx.send(CreateReply::default().embed(embed)).await?; */

    let string_refs: Vec<&str> = strings.iter().map(|s| s.as_str()).collect();

    // Convert Vec<&str> to &[&str]
    let pages: &[&str] = &string_refs;
    poise::samples::paginate(ctx, pages.try_into()?);
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
async fn upload_score(
    ctx: Context<'_>,
    #[description = "Speed of the score"] speed: u16,
    #[description = "Name of the layout"]
    #[autocomplete = "autocomplete_layout"]
    layout: String,
) -> Result<(), Error> {
    let db_path = std::env::var("GARFDB_PATH").unwrap_or("/var/lib/garf/scores.db".into());
    let pool = SqlitePool::connect(&format!("sqlite:{}", db_path)).await?;
    let user_id = ctx.author().id.to_string();

    // Get the LayoutId for the given layout name
    let layout_id = sqlx::query(
        r#"
        SELECT LayoutId FROM layout WHERE Name = ?1
        "#,
    )
    .bind(&layout)
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("LayoutId");

    let delete = sqlx::query(
        r#"
        DELETE FROM Score WHERE LayoutId = ?1 AND User = ?2
        "#,
    )
    .bind(&layout_id)
    .bind(&user_id)
    .execute(&pool)
    .await?;

    // Insert the score
    sqlx::query(
        r#"
        INSERT INTO score (LayoutId, User, Speed)
        VALUES (?1, ?2, ?3)
        "#,
    )
    .bind(layout_id)
    .bind(user_id)
    .bind(speed)
    .execute(&pool)
    .await?;

    ctx.say("Score uploaded successfully!").await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let token = std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");
    let intents = serenity::GatewayIntents::non_privileged();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![leaderboard(), upload_layout(), upload_score(), help()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {})
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}

#[poise::command(slash_command, prefix_command)]
async fn help(ctx: Context<'_>) -> Result<(), Error> {
    let message = "Garf is a bot written to keep track of the highest scores in AKL, and the layouts used. To see the leaderboard, use `/leaderboard`. To put in your own scores, use `/upload_score` with your layout and speed. Feel free to upload your top scores on whatever layouts you like, even Qw\\*rty and Dv\\*rak. If the command returns an error, the layout probably isn't uploaded yet. To upload a layout, use `/upload_layout` with the layout name, the creator (@cmini if the creator isn't here), whether the layout uses magic and/or thumb alpha, and the main focus of the layout, like roll or alt for example. To get the leaderboard filtered by these properties, you can use `/leaderboard` with extra arguments. You can also view scores beyond the top 10 with the `page` argument, and I'm working on improving it";

    let embed =
        CreateEmbed::new()
            .title("Welcome to Garf Bot!")
            .field("What this is for", message, false);
    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}
/* async fn paginate<U, E>(
    ctx: crate::Context<'_, U, E>,
    pages: &[&str],
) -> Result<(), serenity::Error> {
    // Define some unique identifiers for the navigation buttons
    let ctx_id = ctx.id();
    let prev_button_id = format!("{}prev", ctx_id);
    let next_button_id = format!("{}next", ctx_id);

    let embed = CreateEmbed::new()
        .title("Leaderboard")
        .field("Scores", message, false);
    // Send the embed with the first page as content
    let reply = {
        let components = serenity::CreateActionRow::Buttons(vec![
            serenity::CreateButton::new(&prev_button_id).emoji('◀'),
            serenity::CreateButton::new(&next_button_id).emoji('▶'),
        ]);

        crate::CreateReply::default()
            .embed(embed)
            .components(vec![components])
    };

    ctx.send(reply).await?;

    // Loop through incoming interactions with the navigation buttons
    let mut current_page = 0;
    while let Some(press) = serenity::collector::ComponentInteractionCollector::new(ctx)
        // We defined our button IDs to start with `ctx_id`. If they don't, some other command's
        // button was pressed
        .filter(move |press| press.data.custom_id.starts_with(&ctx_id.to_string()))
        // Timeout when no navigation button has been pressed for 24 hours
        .timeout(std::time::Duration::from_secs(3600 * 24))
        .await
    {
        // Depending on which button was pressed, go to next or previous page
        if press.data.custom_id == next_button_id {
            current_page += 1;
            if current_page >= pages.len() {
                current_page = 0;
            }
        } else if press.data.custom_id == prev_button_id {
            current_page = current_page.checked_sub(1).unwrap_or(pages.len() - 1);
        } else {
            // This is an unrelated button interaction
            continue;
        }

        // Update the message with the new page contents
        press
            .create_response(
                ctx.serenity_context(),
                serenity::CreateInteractionResponse::UpdateMessage(
                    serenity::CreateInteractionResponseMessage::new()
                        .embed(serenity::CreateEmbed::new().description(pages[current_page])),
                ),
            )
            .await?;
    }

    Ok(())
} */
