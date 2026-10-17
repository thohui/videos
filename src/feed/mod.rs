use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::Deserialize;

pub mod task;

const CHANNEL_FEED_URL: &str = "https://www.youtube.com/feeds/videos.xml";
const MAX_FEED_BYTES: usize = 1024 * 1024; // 1 MiB

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("building feed http client")
});
pub fn channel_id_from_url(input: &str) -> Result<String> {
    let input = input.trim();
    let id = match input.split_once("/channel/") {
        Some((_, rest)) => rest.split(['/', '?', '#']).next().unwrap_or_default(),
        None => input,
    };
    ensure!(
        id.len() == 24
            && id.starts_with("UC")
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "not a YouTube channel id or /channel/ URL: {input:?}"
    );
    Ok(id.to_owned())
}

pub fn parse(xml: &[u8]) -> Result<Feed> {
    quick_xml::de::from_reader(xml).context("parse feed")
}

#[derive(Debug, Clone, Deserialize)]
pub struct Feed {
    #[serde(rename = "channelId")]
    pub channel_id: String,
    pub title: String,
    #[serde(rename = "entry", default)]
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    #[serde(rename = "videoId")]
    pub video_id: String,
    #[serde(rename = "channelId")]
    pub channel_id: String,
    pub title: String,
    pub link: Link,
    pub author: Author,
    pub published: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    #[serde(rename = "group")]
    pub media: Media,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Link {
    #[serde(rename = "@rel")]
    pub rel: String,
    #[serde(rename = "@href")]
    pub href: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Author {
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Media {
    pub description: String,
    pub thumbnail: Thumbnail,
    pub community: Community,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Thumbnail {
    #[serde(rename = "@url")]
    pub url: String,
    #[serde(rename = "@width")]
    pub width: u32,
    #[serde(rename = "@height")]
    pub height: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Community {
    pub statistics: Statistics,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Statistics {
    #[serde(rename = "@views")]
    pub views: u64,
}

pub async fn fetch_channel(channel_id: &str) -> Result<Feed> {
    let mut resp = CLIENT
        .get(CHANNEL_FEED_URL)
        .query(&[("channel_id", channel_id)])
        .send()
        .await
        .context("fetch feed")?;
    let url = resp.url().clone();
    ensure!(
        resp.status().is_success(),
        "fetch feed {url}: {}",
        resp.status()
    );

    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.context("read feed")? {
        body.extend_from_slice(&chunk);
        ensure!(
            body.len() <= MAX_FEED_BYTES,
            "feed {url} is larger than {MAX_FEED_BYTES} bytes"
        );
    }
    parse(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_FEED: &str = r#"<feed xmlns:yt="http://www.youtube.com/xml/schemas/2015" xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
<link rel="self" href="http://www.youtube.com/feeds/videos.xml?channel_id=UCODHrzPMGbNv67e84WDZhQQ"/>
<id>yt:channel:ODHrzPMGbNv67e84WDZhQQ</id>
<yt:channelId>ODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>fern</title>
<link rel="alternate" href="https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2020-09-24T20:04:44+00:00</published>
<entry>
<id>yt:video:4zxG_oUxBg4</id>
<yt:videoId>4zxG_oUxBg4</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>The Scariest Building in Europe</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=4zxG_oUxBg4"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-10-02T19:20:48+00:00</published>
<updated>2026-10-05T12:19:20+00:00</updated>
<media:group>
<media:title>The Scariest Building in Europe</media:title>
<media:content url="https://www.youtube.com/v/4zxG_oUxBg4?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i1.ytimg.com/vi/4zxG_oUxBg4/hqdefault.jpg" width="480" height="360"/>
<media:description>
Get 20% off DeleteMe with code FERN and protect your privacy: https://joindeleteme.com/FERN (ad) Join our newsletter: https://roots.watchfern.com/ This is the story of a controversial construction project in the heart of London. Behind it are growing fears over espionage, repression and the safety of thousands of people. Special thanks to Q Misell from the Max Planck Institute for Informatics and Carmen Lau. Sources: https://docs.google.com/document/d/1j6mh2oGEjdOowlaboO738kjjvYB4lP7T/edit?usp=drive_link&amp;ouid=100201015226505319360&amp;rtpof=true&amp;sd=true Music list: Epidemic Sound: The Mysterious Cave - Ruiqi Zhao String Quartet No. 2 in D Major, K. 155_ III. Molto Allegro - Wolfgang Amadeus Mozart Dinner at Versailles - Cercles Nouvelles Weapon of Choice - Fabien Tell Yessir - Bonkers Beat Club Thyone - Ben Elson Ghosting - Christoffer Moe Ditlevsen Crystal Glimmer - Amber Glow Bad Vibrations - Stefan Ekstrom Espionage - Deskant Tracker - Christoffer Moe Ditlevsen Lennon Hutton - ES_Mindscape Lens Distortions: Unbroken - For all Times Bending Time - Dimensions There’s a Story - All Things Considered Sleeping Giants - Decades Persona Non Grata - Take the Crown Exile - Dimensions Never Waver - Threshold Overcoming - Against All Odds Ice Climb - Modern Explorers Difficult - Beginnings Inner Steel - Go Forth Foundations - Prototype Edifice - Beginnings Artlist: Sam Lux - Beyond the Hill Kashido - Lullaby Aves - Bumpin Aves - Rainbow City Facing Challenges - Underscore. Tom Fox: Growing Current Another Wave New Formalities Rush of Troops _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="37551" average="5.00" min="1" max="5"/>
<media:statistics views="1825996"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:WOOwVvYoq5I</id>
<yt:videoId>WOOwVvYoq5I</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>The Most Disgusting Influencer Campaign of 2026</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=WOOwVvYoq5I"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-09-23T19:37:52+00:00</published>
<updated>2026-10-01T14:40:13+00:00</updated>
<media:group>
<media:title>The Most Disgusting Influencer Campaign of 2026</media:title>
<media:content url="https://www.youtube.com/v/WOOwVvYoq5I?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i4.ytimg.com/vi/WOOwVvYoq5I/hqdefault.jpg" width="480" height="360"/>
<media:description>
Turn your idea into a real business. Start your Shopify store today at https://shopify.com/fern (ad) Join our newsletter: https://roots.watchfern.com/ This is the story of how journalists uncovered a secretive social media campaign designed to lure young Americans to the Polymarket prediction market platform. It’s largely based on investigations by The Wall Street Journal and Politico. Please check them out: https://www.wsj.com/business/media/polymarket-social-media-bets-prediction-market-441cdeb5 https://www.politico.com/news/2026/06/05/polymarket-paid-political-influencers-00932789 Watch Christophe’s Tunnel Vision video about prediction markets: https://youtu.be/E_V7m_lnybY?si=WZK4CIgd2JZtY_NB Special thanks to Katherine Long at The Wall Street Journal plus Maya Kaufman and Jason Beeferman at Politico. Also thanks to Ted Bradford. Sources: https://docs.google.com/document/d/1JgxiXi_2MymH2aj_MPpoOcD9hps_sr0CiutSd2BmtE8/edit?usp=sharing Music list: Artlist: Jeremy Chontow - Hive Mind Colten Tyler Williams - Gathering Naski - Hunt Semo - Timepiece Damon Power - All the Way Tom Fox: Pointelle - Trailer Version False Treaty Funky Beat Bed Curious Piano 1 Urgent Times Marksmen Epidemic Sound: Forces of Nature - Marten Moses The Search for More - Alec Slayne Pardon My French - Eva Hummingbird Hairpin Loop - Marten Moses Trace - Lennon Hutton Linkage - Marten Moses Finding Dawn - Marten Moses The One Who Almost Got Away - Marten Moses Disappearances - Marten Moses Lens Distortions: Glacier Landing - Bass _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="97349" average="5.00" min="1" max="5"/>
<media:statistics views="6633944"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:d61n1isTfeo</id>
<yt:videoId>d61n1isTfeo</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>The Hunt for El Mencho</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=d61n1isTfeo"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-09-16T16:51:30+00:00</published>
<updated>2026-10-02T16:01:59+00:00</updated>
<media:group>
<media:title>The Hunt for El Mencho</media:title>
<media:content url="https://www.youtube.com/v/d61n1isTfeo?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i1.ytimg.com/vi/d61n1isTfeo/hqdefault.jpg" width="480" height="360"/>
<media:description>
Taking care of your health just got easier, thanks to our sponsor Zocdoc. Start here: https://zocdoc.com/fern (ad) Join our newsletter: https://roots.watchfern.com/ El Mencho may be the most powerful and ruthless drug lord in the world. His cartel has taken on the Mexican state and built a global criminal empire. How did he become so powerful? Why has no one been able to stop him? And what happens if he is finally caught? Much of this video is based on the book “CJNG: A Quick Guide to Mexico's Deadliest Cartel” by Chris Dalby. Link: https://www.amazon.com/dp/9083423913?lv=shuf&amp;channelId=500&amp;plpRedirect=mhFallback Sources: https://docs.google.com/document/d/1XV1xVQtKZZ7dGaKoWA9T6ZXBrCSHopK7B5PF8OsGnBI/edit?usp=sharing Music list: Epidemic Sound: ES_Desert Whispers - Tellsonic ES_Rapid Surge - Hampus Naeselius ES_Tunnels - DEX 1200 ES_Out of Control - Stefan Ekstrom ES_TUNDRA - Tigerblood Jewel ES_Ceres - Lennon Hutton ES_Preserve - Anthony Earls ES_Snowy Mountains - Toby Tranter ES_Misty Land - Headlund ES_Rhythmania 1 - August Wilhelmsson ES_The Choice - Alec Slayne ES_Margin - Daniella Ljungsberg ES_A Breach in the Wall - Dream Cave ES_Lost Senses - Sage Oursler ES_Kill Protocol - Dream Cave ES_Paranormal Activities - Jon Bjork ES_Forsaken - Luba Hilman ES_NARCO - BLUE STEEL ES_In Obscurity Lies the Gate - Christoffer Moe Ditlevsen ES_Bird Cage - Dylan Sitts ES_Automatic City - Hampus Naeselius ES_Infinity Pool &amp; Pool Tables - Mythical Score Society ES_Troglodytes - T-Shirts &amp; Sweats ES_Liquified - Jon Sumner ES_The Big Clash - Dream Cave ES_Something in the Water - Czar Donic ES_Procession of Time - Rand Aldo ES_Born Sinister - Christoffer Moe Ditlevsen ES_Silk Road Caravan - Constellate ES_Hide - Lennon Hutton ES_My Domain - Non-State Actor ES_In Obscurity Lies the Gate - Christoffer Moe Ditlevsen ES_Neo Two K Six - SINY ES_The Loose Ends - Craft Case ES_Definite Steps - Franz Gordon ES_Slammed Shut - Craft Case ES_Soothing Serenade - Anna Dager ES_Enter the Facility - Eden Avery ES_Trapped in Confusion - Eludent ES_Iron Dome - Hampus Naeselius ES_Oppression - Jon Sumner ES_Echoes of Eternity - Tellsonic Artlist: River Lume - Cali Nocturna Ilan Bar-Lavi - Por Que - Instrumental version _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="77759" average="5.00" min="1" max="5"/>
<media:statistics views="7732359"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:hK8kzWOBi-U</id>
<yt:videoId>hK8kzWOBi-U</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>The FBI's Most Valuable Agent</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=hK8kzWOBi-U"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-09-09T14:00:16+00:00</published>
<updated>2026-10-04T10:50:04+00:00</updated>
<media:group>
<media:title>The FBI's Most Valuable Agent</media:title>
<media:content url="https://www.youtube.com/v/hK8kzWOBi-U?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i1.ytimg.com/vi/hK8kzWOBi-U/hqdefault.jpg" width="480" height="360"/>
<media:description>
Epidemic Sound has been part of our toolkit for years. If you want to try it yourself, click on our link and use code fern60 at checkout to get 60% off for 2 months: https://share.epidemicsound.com/fernSEP26 *Offer valid until Oct 15th (ad) Join our newsletter: https://roots.watchfern.com/ This is the story about Donnie Brasco, the undercover agent who infiltrated the New York Mafia. Much of this video is based on Joe Pistone’s own accounts of his time undercover, particularly his books: Donnie Brasco: My Undercover Life in the Mafia https://www.amazon.com/s?k=Donnie+Brasco+My+Undercover+Life+in+the+Mafia+Joe+Pistone The Way of the Wise Guy https://www.amazon.com/s?k=The+Way+of+the+Wise+Guy+Joe+Pistone Donnie Brasco: Unfinished Business https://www.amazon.com/s?k=Donnie+Brasco+Unfinished+Business+Joe+Pistone For more, listen to Deep Cover: The Real Donnie Brasco by Jam Street Media: https://open.spotify.com/show/3Mf4oliHd7xlfB3LNn0ym4?si=9775aba2ebfd44ff Credit for the chart at 07:29 goes to @Roose1120. Check out his Reddit profile: https://www.reddit.com/user/R0ose/ Corrections: 0:17 'Willwaukee' is a typo, but fern does acknowledge and celebrate the strong will of the good people of Wisconsin. Go Pack! 33:33 The correct spelling is Tony Rossi. Sources: https://docs.google.com/document/d/16yS3scVXYZiVeQ9uE952r9u_iLN3W1uBXEHZnHuGq1k/edit?usp=sharing Music list: Music by Epidemic Sound: Troublesome Future - Blackout Memories Hiding in the Shadows - Rymdklang Soundtracks The Shadow - Christoffer Moe Ditlevsen Unseen - Lennon Hutton Awaiting Backup - Experia And Then She Said Yes - Christian Andersen Code of Violence - Dian Shuai Forest of Eternity STEMS - Polar Nights Booh Ya - Tigran Viken Weird Uncle - spring gang Joe's Garage - Rymdklang Soundtracks The Beginning of the End - Stefan Ekstrom Handle It - Wendel Scherer Blood Money - Hampus Naeselius Can They See Us - Experia Partner of Doom - Walt Adams The Burnout - Gerhard Feng Purple, Pink and Orange - OTE Digging Deeper - Stefan Ekstrom Gotta Give it Back (Instrumental Version) - Par Hagstrom We Will Never Die (Instrumental Version) - Par Hagstrom Urban Stories - Chill Cole A Little More Shine to That - Auxjack The Vice - Hampus Naeselius Protocol Violation - Ethan Sloan Dreamers and Believers (Instrumental Version) - Par Hagstrom Late but That's Great - Golden Age Radio Minutes of Significance - Charles Holme Lemon Squeezy - Martin Landstrom Darkness Falls - Etienne Roussel Transmitting - Ethan Sloan Leather Feather - Lennon Hutton (Stay) True (Instrumental Version) - Lars Eriksson Ghosting - Christoffer Moe Ditlevsen Enter the Facility - Eden Avery We Are on Our Own - Christian Andersen Wind Cave - Martin Klem Coffee in Bristol - Farrell Wooten Yellow Tape - Piper Ezz Carnivores Align - Christian Andersen Zero Hour - Hampus Naeselius With Music on Repeat (Instrumental Version) - Victor Lundberg Absolute Power - Hampus Naeselius Orcus - Lennon Hutton March of the Dead - Max Anson Tumbleweed - Roots and Recognition Wasted Silence - Max Anson TUNDRA - Tigerblood Jewel Inhale - Andreas Dahlback 2nd Street Shuffle - Roots and Recognition I Wanna Burn with You (Instrumental Version) - Par Hagstrom Marker 0 - Luba Hilman Danger Ahead - Etienne Roussel Grit and Danger - Craft Case Out of Control - Stefan Ekstrom Easy Challenge - Martin Landstrom Abandoned Village - Lennon Hutton Intercept - Hampus Naeselius Sudden Discomfort - Ethan Sloan Recession - Ethan Sloan Unease - Craft Case Ufo - Wendel Scherer Surfing Astray - Johan Glossner Danger Street - Max Anson Thick Air - Jon Sumner The Grim Reaper (Instrumental Version) - Roof In Continuum - Dream Cave When They All Left - Christian Andersen Miasma Shapes - Elin Piel Chrome and Steel - American Legion Cold Sweat - Craft Case Active Entrance - Ethan Sloan Stop Snitchin' - Isaac Larson Bionic - Ethan Sloan Under the Dome - Philip Ayers Consumed by Darkness - Christoffer Moe Ditlevsen Wolf Signal - Ethan Sloan Kill Switch - Jon Sumner All God's Children (Instrumental Version) - Par Hagstrom Heroic Actions - Wendel Scherer Foreign Entity - Calm Shores And Then - DEX 1200 So Long and Goodnight - Tigerblood Jewel Myself and I - Niklas Gabrielsson with Martin Landstrom &amp; His Orchestra Additional 3D models by: Albert Buscio McPato HikiMore Radint Arrangemonk Icevanilla dohnjoe Charckes maregajavier akneeee DaengNu Daniel Zhabotinsky NobleCrowDev Arsen Ismailnov Elbolillo Wardster smooth998 Moustache_Cat sumitmangela pixelbutterfly _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="128925" average="5.00" min="1" max="5"/>
<media:statistics views="6924146"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:-Gnrp_caPvo</id>
<yt:videoId>-Gnrp_caPvo</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>The Death of Educational Content on YouTube</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=-Gnrp_caPvo"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-09-02T17:13:32+00:00</published>
<updated>2026-10-03T07:57:45+00:00</updated>
<media:group>
<media:title>The Death of Educational Content on YouTube</media:title>
<media:content url="https://www.youtube.com/v/-Gnrp_caPvo?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i2.ytimg.com/vi/-Gnrp_caPvo/hqdefault.jpg" width="480" height="360"/>
<media:description>
FlexiSpot Labour Day Sale is here - up to 65% off sitewide! Use code FERN50 to get an extra $50 off on all products. Check out the E7 Pro Standing Desk: 🇺🇲 https://go.flexispot.com/fern-us 🇨🇦 https://go.flexispot.com/fern-ca Full sale: https://go.flexispot.com/fern-sale (ad) Join our newsletter: https://roots.watchfern.com/ Countless AI slop channels are relentlessly copying our work. So one day we decided: enough is enough. We went undercover to investigate who’s behind them and what this flood of AI slop means for the future of educational content on YouTube. Many thanks to Dr. Akhil Bhardwaj, Prof. Dr. Andrea Löw, neo, Memeable Data and The Present Past for their valuable insights. Check out their channels: neo: https://www.youtube.com/@neoexplains Memeable Data: https://www.youtube.com/@memeabledata The Present Past: https://www.youtube.com/@ThePresentPast_ IMPERIAL: https://www.youtube.com/@IMPERIALYT/videos Sources: https://docs.google.com/document/d/14c9vJUltQuvE1tyC4zC61csEhg2GNGxymlQQzmLTt4k/edit?usp=sharing Music list: Intro: Déjà-vu - N01R Epidemic Sound: Grieg 2 Elegiac Melodies, Op. 34_ No. 2, The Last Spring - Odyssey Orchestra Cosmic Slumber - Valante Beco das Garrafas - El Equipo Del Norte SPHINX - Blue Steel Jackpot - Blue Steel Itsumo - Valante Bionic - Ethan Sloan Bad Vibrations - Stefan Ekstrom Woodwind Reviere - Daniella Ljungsberg A Truth Unwanted - Stefan Ekstrom Big Smile Boogie - Speedy The Spider Interstellar - Blue Steel Can't Handle It - Ryan James Carr Lens Distortions: Variables Prolific Prototype Awake Foundations Tom Fox: Forever Conflict Uptight Deeper Sea Attractive Forces (Variation 2) New Formalities Technically Correct Minimal Deep Synth Artlist: DELNOVA - Follow the River Ian Post - New World Tamuz Dekel - The Shoulder Tap Matooma - Dont Look Back _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="270240" average="5.00" min="1" max="5"/>
<media:statistics views="10758418"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:65hjeAq6Oq0</id>
<yt:videoId>65hjeAq6Oq0</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>McDonald's Darkest Secret</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=65hjeAq6Oq0"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-08-26T19:35:11+00:00</published>
<updated>2026-10-02T14:55:07+00:00</updated>
<media:group>
<media:title>McDonald's Darkest Secret</media:title>
<media:content url="https://www.youtube.com/v/65hjeAq6Oq0?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i3.ytimg.com/vi/65hjeAq6Oq0/hqdefault.jpg" width="480" height="360"/>
<media:description>
If you have a project idea, start building here: https://lovable.link/fern (ad) Join our newsletter: https://roots.watchfern.com/ This is the story of McDonald’s iconic Styrofoam burger box and how it became one of the most controversial pieces of fast-food packaging ever made. Special thanks to Saabira Chaudhuri for her time and insights. The autobiography Barefoot to Billionaire by Jon M. Huntsman Sr. served as a key source for telling his story in this video. Corrections: 21:56: The logo shown for the Environmental Defense Fund (EDF) is incorrect. The correct EDF logo can be found on their official website: https://www.edf.org/where-we-work Sources: Source Vault: https://fern.deals/source-vault (made with Lovable) https://docs.google.com/document/d/12Gq41d_ASOkciBzi7iLByW8iSFrGgRsdyOXfDPQKnDg/edit?usp=sharing Music list: Artlist: Lance Conrad - Kill or Be Killed Showdown lumine wave - Violin Resonance Tony Balluff - Under the Tent Max Hixon - On the Prowl.wav Dan Ayalon - Promised Land Out of Flux - What The Dog Doing Ran Raiten - Silent Assassin Epidemic Sound - try 30 days for free via our affiliate link: https://fern.deals/ES (ad) Doing the Twist - OTE Weak Spot - Jon Bjork Like Moths to a Flame - Alec Slayne 77 Cycles - Lennon Hutton A State of Mind - Nebulae Echoes of Eternity - Tellsonic Aquatic Flower Dance - Ryan James Carr.wav One Must Go - Alec Slayne Tempting Decisions - Wendel Scherer Subconscious - Nihoni Tracker - Christoffer Moe Ditlevsen Pattern View - David Celeste Impossible Theory - Rachel Sandy Estimations - Christoffer Moe Ditlevsen Divisive Alliance - Jon Björk Idler - Prozody Mindscape - Lennon Hutton Colony - Taelimb Grey Matter - Enigmanic Pattern View - David Celeste Out There - Out To The World _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="43649" average="5.00" min="1" max="5"/>
<media:statistics views="2637287"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:zy6ZN1qM3sY</id>
<yt:videoId>zy6ZN1qM3sY</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>The Unsolved Murder in Room 1046</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=zy6ZN1qM3sY"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-08-19T14:00:03+00:00</published>
<updated>2026-09-29T11:08:08+00:00</updated>
<media:group>
<media:title>The Unsolved Murder in Room 1046</media:title>
<media:content url="https://www.youtube.com/v/zy6ZN1qM3sY?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i3.ytimg.com/vi/zy6ZN1qM3sY/hqdefault.jpg" width="480" height="360"/>
<media:description>
Use our code FERN10 to get 10% off your Manta Sleep masks here: https://fern.deals/manta/FERN10 (ad) Join our newsletter: https://roots.watchfern.com/ This is the story of Room1046, one of America’s strangest unsolved murders. Thank you to Dr. John Arthur Horner of the Kansas City Public Library - for his research into this case. Corrections: 11:06 The clock reads “12 PM,” but it should be “12 AM.” Sources: https://docs.google.com/document/d/1ON1rbb4SSPp4T-pYvIv5JTX50ru9bDaWusJ_ajC_RFA/edit?usp=share_link Music list: Tom Fox: Socialite What If He's Right Artlist: Alon Peretz - Pleasure and Pain Alon Peretz - A Forest Dark Alon Peretz - Fire and Ice Rotem Cinamon - Dusk Bunker Buster - Mouse Hunter Brian Claxton - We Really Dont Know Eli Benacot - The Play - Instrumental version The Magnetic Buzz - Keep Your Head Down FableForte - Riddles Jeremy Chontow - A Villains Theme Epidemic Sound: Mr Wilson Visits - Mary Riddle The Blue Tiger - Guto Lucena They Say I'm Mad - Mary Riddle Cold Nights - Kikoru The Haunted House - John Abbot The Detective's Story - Ruiqi Zhao They Say I'm Mad - Mary Riddle A Grey Day - Guto Lucena Weathered Eyes - Alec Slayne Leaving Traces - Anna Landstrom Additional music composed by Marcel Becker-Neu _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="149575" average="5.00" min="1" max="5"/>
<media:statistics views="5705205"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:a4EcjsnM6ro</id>
<yt:videoId>a4EcjsnM6ro</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>How China's Biggest Scammer Got Caught</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=a4EcjsnM6ro"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-08-12T14:00:13+00:00</published>
<updated>2026-10-05T05:56:38+00:00</updated>
<media:group>
<media:title>How China's Biggest Scammer Got Caught</media:title>
<media:content url="https://www.youtube.com/v/a4EcjsnM6ro?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i2.ytimg.com/vi/a4EcjsnM6ro/hqdefault.jpg" width="480" height="360"/>
<media:description>
Check out Emergent if you have an idea you've been meaning to build: https://fern.deals/emergent (ad) Join our newsletter: https://roots.watchfern.com/ This is the rise and fall of crypto queen Zhimin Qian. After taking money from more than 120,000 people, she flees China and lives in luxury across Europe - until one small mistake brings everything crashing down. Our reconstruction of Zhimin Qian’s biography is largely based on a detailed profile published by the Chinese magazine China Newsweek. Sources: https://docs.google.com/document/d/1avYw4P5153-X3j8922GBpIIJXll7tffIyinJ5Ea8Gmc/edit?usp=sharing Music list: Artlist: Flint - No Trust - Flint Remix DELNOVA - Magnify Matthias Förster - With Odin's Help mee - Telephones DaniHaDani - Dimension Flint - i need a break 2050 - Game Over Oliver Michael - Setup Ian Post - Secret Garden Anthony Vega - The Poker Player Jasmine J Walker - The Queen Out of Flux - Eclipse Audiio: ATYYA Remixes - Breath Of Me LiamBack - CapitalCorp Genelex - Comfort Zone Guzer - Floating Godmode feat Lavito - Last Man Standing LivingForce - Road to Danger JLZ - LOST PLUGINSSSSSS songstate - too-hard Joel Bear - Organic Dance Raphael Novarina - Shadow Man Epidemic Sound: Intro for Legends - Bonkers Beat Club Slow Rhodes - Blue Saga No More Questions - Experia Crunched - Bonkers Beat Club Farewell Parade - DEX 1200 Bite Marks - Eludent Darkest of Demons - Hampus Naeselius SIT DOWN - Tigerblood Jewel Lens Distortions: Threshold - Threshold The Last City - Heart of the City Threshold - Prep Work Force Multiplier - Knife's Edge Force Multiplier - Full Contact Force Multiplier - In the Zone Threshold - Stress Test Into the Storm - Facing Challenges _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="55593" average="5.00" min="1" max="5"/>
<media:statistics views="2775840"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:Eo0JQLIG6hA</id>
<yt:videoId>Eo0JQLIG6hA</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>Exposing a $1,900,000,000 Pharma Company</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=Eo0JQLIG6hA"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-08-05T19:30:55+00:00</published>
<updated>2026-10-02T23:12:53+00:00</updated>
<media:group>
<media:title>Exposing a $1,900,000,000 Pharma Company</media:title>
<media:content url="https://www.youtube.com/v/Eo0JQLIG6hA?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i2.ytimg.com/vi/Eo0JQLIG6hA/hqdefault.jpg" width="480" height="360"/>
<media:description>
Try Brilliant for free, plus get 20% off an annual Premium subscription at https://brilliant.org/fern (ad) Join our newsletter: https://roots.watchfern.com/ This is the story of a German pharmaceutical company, a billion-dollar opioid business, and a crisis spanning multiple continents. Is a new opioid crisis looming — one made in Germany? This video is based on original reporting by The Examination, a nonprofit newsroom investigating preventable health threats. More on this story: https://www.theexamination.org/articles/gruenenthal-pushed-its-latest-opioid-as-a-safer-option-people-around-the-world-got-hooked?utm_source=youtube&amp;utm_medium=referral&amp;utm_campaign=creator_mention&amp;utm_content=fern_gruenenthal_opioid Explore the investigation and full series: https://www.theexamination.org/series/world-of-pain Follow The Examination: Website: https://theexamination.org YouTube: https://www.youtube.com/channel/UCnXpJDgzBRbNcjnNTJpa42Q Newsletter: https://theexamination.org/newsletter You can find Tanja Albroscheit’s petition here: https://www.change.org/p/dr-klaus-rei... Many thanks as well to Maria Christoph from Paper Trail Media. Her in-depth article for the German magazine Spiegel is available here: https://www.spiegel.de/panorama/gesel... To better understand the situation in West Africa, make sure to watch this ARTE documentary: https://www.youtube.com/watch?v=8r3E_-O44io Corrections: 0:01: "Australien” should read “Australia". Sources: https://docs.google.com/document/d/1oemg0YMbFv4QM5WNVJHGdGOHg64W-d52fyubEhj77xU/edit?usp=sharing Music list: Epidemic Sound: Traitors - Wendel Scherer Suicidal Drivestyle - Hysics Half-Time Break - Experia Devils On The Doorstep 2 - Fredrik Ekström Hidden Room - Anthony Earls Mind Your Step - Craft Case Stop Snitchin’ - Isaac Larson Inside Light - Jay Varton I Have a Plan, Mr Norton - Arthur Benson Zone - Jay Varton Me vs Me - Jharee Quell Fear - Jon Björk All Day - Tilden Parc One Last Drama - Philip Ayers We Have to Go Back There - Christian Andersen Ends Don't Meet - Kouba 22c - Farrell Wooten Look to the Sky - Experia Let This Be a Lesson - Eludent Stars and Neutrinos - Christian Andersen After the Fall - Max Anson Rainy Landscapes - Farrell Wooten Stormkeeper - Robert Ruth Anemone - Marten Moses The Replica - Toby Tranter People Falling Down - Arthur Benson Victims &amp; Crimes - Skrya Adaption - Out To The World Trace It - Skrya Cold Sweat - Craft Case Peace on the Horizon - Krithi Tension 2 - Fredrik Ekström Margin of Error - Farrell Wooten Red Sun Rising - Edward Karl Hanson In Their Galaxy - Christian Andersen I'm the Villain - Dream Cave True Forgiveness - Jon Björk Villanueva - Vendla Fall Afoul - Jon Sumner Tracker - Christoffer Moe Ditlevsen Secret Admirer - T. Morri Critical Thinking - Philip Ayers Enter The Night 5 - Fredrik Ekström Dark Secrets - Stefan Ekström The Mole - Christoffer Moe Ditlevsen The Call of the Wild - Victor Lundberg Tragic Departure - Atlas Kind Call On Me (Instrumental Version) - Tape Machines Finger Roll - Bonkers Beat Club Danger Ahead - Etienne Roussel _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="56743" average="5.00" min="1" max="5"/>
<media:statistics views="3205268"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:V6d8Yy7Ji68</id>
<yt:videoId>V6d8Yy7Ji68</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>Hansa: The Infiltration of the Dark Web</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=V6d8Yy7Ji68"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-07-29T14:00:37+00:00</published>
<updated>2026-10-05T11:34:23+00:00</updated>
<media:group>
<media:title>Hansa: The Infiltration of the Dark Web</media:title>
<media:content url="https://www.youtube.com/v/V6d8Yy7Ji68?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i3.ytimg.com/vi/V6d8Yy7Ji68/hqdefault.jpg" width="480" height="360"/>
<media:description>
Odoo's first app is free for life - and starting at just $24.90/month, you get access to all applications. Check out everything Odoo has to offer here: https://fern.deals/odoo/C8L (ad) Join our newsletter: https://roots.watchfern.com/ This is the story of one of the boldest international police investigations of all time, likely the largest trap in Darknet history. This is Operation Bayonet. Links to WIRED’s reporting: https://www.wired.com/story/hansa-dutch-police-sting-operation/ https://www.wired.com/story/alphabay-series-part-3-alpha-male/ https://www.wired.com/story/alphabay-series-part-4-face-to-face/ Sources: https://docs.google.com/document/d/1T7s65rPHMH6H0ZuMxFhGMcCwXXh4RjBxJPOow3VxTkY Music list: Title Song: Marcel Becker Neu - Red Riding Hood Artlist: Stanley Gurvich - Absolution wav Yarin Primak - Buss It Loop Danny Shields - Vibrations Jimmy Svensson - The Darkest Day Sam Lux - Unbreakable Code Quinten Coblentz - Babylon Out of Flux - SNIPERBUNKER Flint - Don't Open Oliver Michael - Setup Quinten Coblentz Just the Beginning Out of Flux - CLASH OF THE MACHINES Flint - March the Machine Evgeny Bardyuzha - High Contrast Fluid Epidemic Sound: Otarefsun - par Suicidal Drivestyle - Hysics Metal under Tension - Hampus Naeselius Blackhat - Sonum Voigt Kampff - Martin Baekkevold Crossing the Border - Hampus Naeselius Space Noise - Martin Klem It's Just Noise - Blue Saga Lens Distortions: The Unlock - Background Elements Precursor No Half Measures The Unlock Audiio: Genelex - Tensor X Brad Hill - Delivered Distrubance Brad Hill - Darkness at Dawn Yari - See Brad Hill - Dark City Terem - WhereAreYouGoingEP - Fire Trap Kodomo - Three SpherHeat Death Guzer - Floating - Floating Brad Hill - 10 - Dark Static Theatre Of Delays - Theatre Of Delays - Architect Brad Hill - Hollow Machine Theatre Of Delays - Remnant Credits: Executive Producers: Jonas, David, Paul Lühnsdorf Producer: Mona Schier Editorial Supervision: Lucie Schöner, Jonas Script: Julia Fact-Checking: Pascale Müller Voice-Over: Don Directed by: Marcel Becker-Neu Production Coordination: Vera Hartlieb Production Assistant: Philip Kamper Editing: Bernd MSC Schroers 3D Animation: Thomas Overbeeke 2D Animation: Dennis Gajdek Camera: Jannik Nolte First Assistant Camera: Simon Andjic Grading: Adrian Honsberg Titel-Track: Marcel Becker-Neu Sound Design: Bernd MSC Schroers Sound Mix: Studio Soma Cast: Lasse Jonas Rogie, Philip Kemper, Mona Schier, Jeroen Van Hoorn, Lakchih Tong, Simon Andjic, Jannik Nolte, Marcel Becker-Neu _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="132979" average="5.00" min="1" max="5"/>
<media:statistics views="4717943"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:TlioXObelHU</id>
<yt:videoId>TlioXObelHU</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>The Truth About fern</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=TlioXObelHU"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-07-22T14:00:04+00:00</published>
<updated>2026-10-01T08:55:04+00:00</updated>
<media:group>
<media:title>The Truth About fern</media:title>
<media:content url="https://www.youtube.com/v/TlioXObelHU?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i1.ytimg.com/vi/TlioXObelHU/hqdefault.jpg" width="480" height="360"/>
<media:description>
👇 Checkout Revolut via this link &amp; receive €20 / £20 / $20 USD / $40 AUD: https://get.revolut.com/z4lF/fern 18+. New users only. T&amp;Cs apply. Min spend €0,01 / £0,01 / $0,01 USD / $0.01 AUD with debit card required. Promotion ends 30/09/2026. (ad) Join our newsletter: https://roots.watchfern.com/ This is how fern became fern, how we make our videos and what's next. Music list: Artlist: Francesco DAndrea - Rise Again Yaniv Hayoun - Oak Tree Prelude Ian Post - New World IamDayLight - Alaska Jam Man with Roses - To Burn Novembers - Last Train Home Ian Post - Be the Best - Instrumental version Tom Fox: Cause Epidemic Sound: Pettles - Frook Back on Deck (Instrumental Version) - Xavy Rusan The Connection - Lennon Hutton Blacklight - John B. Lund Fallout - Lennon Hutton Unfinished Stories - Lennon Hutton Orbit Expansion - Out To The World Chelsea Reach - Howard Harper-Barnes Lens Distortion: Defining Beliefs - First Principles Sandstorm - Modern Explorers III Revolut is not a bank, banking services are provided by Lead Bank, Member FDIC. Fees may apply. The Revolut USA Prepaid Visa and Prepaid Mastercard are issued by Lead Bank pursuant to licensing by Visa® U.S.A. Inc. and Mastercard International for Mastercard cards. Your funds are FDIC insured up to $250,000 through Lead Bank, Member FDIC. See revolut.com/en-us/ for more details. #Sponsored #ad. Terms and conditions for US customers found: https://www.revolut.com/en-US/legal/partner-sign-up-bonus/. For AU customers, consider PDS &amp; TMD at revolut.com/AU/. Revolut Payments Australia Pty Ltd (AFSL 517589). _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="127732" average="5.00" min="1" max="5"/>
<media:statistics views="2401460"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:xV82vkJ5Pjw</id>
<yt:videoId>xV82vkJ5Pjw</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>Why McDonald's Avoids 1.4 Billion People</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=xV82vkJ5Pjw"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-07-15T15:22:56+00:00</published>
<updated>2026-10-02T22:24:50+00:00</updated>
<media:group>
<media:title>Why McDonald's Avoids 1.4 Billion People</media:title>
<media:content url="https://www.youtube.com/v/xV82vkJ5Pjw?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i1.ytimg.com/vi/xV82vkJ5Pjw/hqdefault.jpg" width="480" height="360"/>
<media:description>
Turn your idea into a real business. Start your Shopify store today at https://shopify.com/fern (ad) McDonald’s conquered the world - but not Africa. Why are the golden M’s so rare across the continent? Corrections: 0:10 The figures shown are in billions of US dollars, not millions. 4:01 The timeline shows incorrect years after 1995. Sources: https://docs.google.com/document/d/1wrUWXweHEZLT_QNEbfAwCr0_DIzqVZLXJFzox7W5uM0/edit?usp=drivesdk Music list: Artlist: FableForte - Riddles Flint - Far from Me Ian Post - Twilight Shadows Lens Distortion: Velos - Tenacity Threshold - Forward Threshold - Inner Fire Dimensions - Order of Magnitude Challengers - Forcing Function Epidemic Sound - try 30 days for free via our affiliate link: https://fern.deals/ES (ad) Farrell Wooten - Out of the Cage Nyck Caution - Come See Me (Instrumental Version) Isaac Larson - Straight Flush Hysics - Wizard Light Alec Slayne - The Search for More Dusty Wheels - Kikoru Impossible Theory - Rachel Sandy Like Moths to a Flame - Alec Slayne 90210 - BLUE STEEL The Man With The Maps Scramble Boots On The Ground _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="54115" average="5.00" min="1" max="5"/>
<media:statistics views="2600524"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:EkLmxmUfgBU</id>
<yt:videoId>EkLmxmUfgBU</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>Inside the Dark World of Poker Cheating</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=EkLmxmUfgBU"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-07-08T16:30:05+00:00</published>
<updated>2026-10-04T21:50:43+00:00</updated>
<media:group>
<media:title>Inside the Dark World of Poker Cheating</media:title>
<media:content url="https://www.youtube.com/v/EkLmxmUfgBU?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i2.ytimg.com/vi/EkLmxmUfgBU/hqdefault.jpg" width="480" height="360"/>
<media:description>
Get an exclusive 15% discount on Saily data plans! Use the code ‘FERNTV’ at checkout. Download the Saily app or go to https://saily.com/ferntv ⛵ (ad) This is the story of an enormous alleged poker scandal. Special thanks to Jason England, Doug Polk, Sarah Patton, and the Nevada Historical Society for their valuable time and expertise. Corrections: 13:42 We show a supposed driver’s license from Yevgeni Gershman. This photo was taken from documents of the US District Court for the Southern District of California which claims this was a driver’s license from Cyprus. However this seems to be untrue. We cannot be sure what type of document this represents or whether it is potentially a fake document. Sources: https://docs.google.com/document/d/1uQ9fJEEip38wIJQelbmUJA6HA3yjq6PsdJhS1Wk08-I/edit?usp=sharing Music list: Artlist: Sydney Miller - In the Office Tonecat Groove · Yonatan Riklis Walz - HellHound Amos Ever Hadani - Blues for Leo Ian Post - Get On Up - Instrumental version Joca Perpignan - No mundo da percussão Anthony Vega - The Poker Player Epidemic Sound - try 30 days for free via our affiliate link: https://fern.deals/ES or use code FERN50 to get 50% off your first 2 months at checkout (ad): Oh Please - Gerhard Feng Bring It Down - Gerhard Feng Sonder Thoughts - The New Fools Rockin' Bastards - Gerhard Feng Play That Funk - Gerhard Feng Movers and Shakers - Elliot Holmes The Cover-Up STEMS DRUMS - White Bones Mysterious Madeleine STEMS DRUMS - Lucas Pittman Audiio: CTRL FREQS - So Hot So Fresh Faded Light - No Limits Faded Light - Revival Daniel Dor - Medium Up Raphael Novarina - DISTINCTION Ghosthood - Steel Pulse (Let’s Go) Roie Shpigler - Blooz Mojo Lake Union - Heavily Cloaked Keepitphilthy - Believe It Tom Fox: Castro's Death Bomb Mr. Suede The Bad Bet Cool Hand Amphibious Jang Chasing A Dream The Heist Castro's Death Bomb _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="87758" average="5.00" min="1" max="5"/>
<media:statistics views="3811629"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:dNGUiWKn6Qs</id>
<yt:videoId>dNGUiWKn6Qs</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>The Kid Who Hacked America</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=dNGUiWKn6Qs"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-06-30T14:32:29+00:00</published>
<updated>2026-09-30T06:41:57+00:00</updated>
<media:group>
<media:title>The Kid Who Hacked America</media:title>
<media:content url="https://www.youtube.com/v/dNGUiWKn6Qs?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i1.ytimg.com/vi/dNGUiWKn6Qs/hqdefault.jpg" width="480" height="360"/>
<media:description>
Click this link https://boot.dev/?promo=FERN and use our code FERN to get 25% off your first payment for boot.dev (ad) This is the story of Kevin Mitnick. Vilified by the media. Hunted by the authorities. Feared as one of the world's most dangerous hackers. This video is largely based on the book “Ghost in the Wires” by Kevin Mitnick. Sources: https://docs.google.com/document/d/19ZtWdhlWgLexcd13GYKBhL82hfmYnApvc2ivSX8lxQc/edit?usp=sharing Music list: Epidemic Sound - try 30 days for free via our affiliate link: https://fern.deals/ES or use code FERN50 to get 50% off your first 2 months at checkout (ad): Farrell Wooten - Hunter Hunted; Steady Tension Hampus Naeselius - 1000 Watts; Replicants; Overdrive pär - This Spell Is Different Gunnar Johnsen - Birthday Party (70s Rock Version) Elliot Holmes - Late Night Flight ELFL - Temple of Runha Zorro - I Only Know Never (Instrumental Version) Gabriel Lewis - Cold War Games Bill Ferngren - Silent Screams Max Anson - Feeling New Ruiqi Zhao - Vroom Vroom Lennon Hutton - Signs; Red Alert Guy Copeland - Monitoring Boundaries Christoffer Moe Ditlevsen - The Stakeout Artlist: Night Rider 87 - K Roie Shpigler - Enchanted Valley SOURWAH - Unbreakable - Instrumental version Jimmy Svensson - Exploit Gal Lev - My Dark Side Katrina Stone - Midas - Instrumental version Russo - Night Search Out of Flux - cdHiddenDir Adv3n7ur35 - Awaken Dawn - Instrumental version Stephen Keech - Black Hole Ian Post - Night Rider Audiio: Ian Mann - Rescue Gone Wrong (Instrumental) Kodomo - Numbers (Kodacrome-Remix) (Instrumental); Unrosive (Instrumental); Three Cycles (Instrumental) Braden Alec Michael - A Moment At Dusk (Instrumental) Tom Fox: Noon Amphibious Unknowns There Is Dirt Chopper _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="84360" average="5.00" min="1" max="5"/>
<media:statistics views="3997044"/>
</media:community>
</media:group>
</entry>
<entry>
<id>yt:video:5pO-MTDW6nU</id>
<yt:videoId>5pO-MTDW6nU</yt:videoId>
<yt:channelId>UCODHrzPMGbNv67e84WDZhQQ</yt:channelId>
<title>Why Putin Is (Almost) Impossible to Kill</title>
<link rel="alternate" href="https://www.youtube.com/watch?v=5pO-MTDW6nU"/>
<author>
<name>fern</name>
<uri>
https://www.youtube.com/channel/UCODHrzPMGbNv67e84WDZhQQ
</uri>
</author>
<published>2026-06-24T16:56:32+00:00</published>
<updated>2026-10-02T13:06:48+00:00</updated>
<media:group>
<media:title>Why Putin Is (Almost) Impossible to Kill</media:title>
<media:content url="https://www.youtube.com/v/5pO-MTDW6nU?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
<media:thumbnail url="https://i2.ytimg.com/vi/5pO-MTDW6nU/hqdefault.jpg" width="480" height="360"/>
<media:description>
Taking care of your health just got easier, thanks to our sponsor Zocdoc. Start here: https://zocdoc.com/fern (ad) Vladimir Putin has a lot of enemies. So how does he protect himself? In this video, we’ll look at four of the Kremlin’s security measures. They tell a story, one about the Russian state and its increasingly paranoid leader. Many thanks to Ilia Rozhdestvenskii for his valuable insights. Sources: https://docs.google.com/document/d/1XNujO85QLOxvhH0Mzx6WOuhQSfBiS22vhwMONgrpbH0/edit?usp=sharing Music list: Epidemic Sound - try 30 days for free via our affiliate link: https://fern.deals/ES (ad) Pressure Drop - Max Anson Mystical Tension - Dream Cave Tracker - Christoffer Moe Ditlevsen Replicants - Hampus Naeselius End of Luck - Max Anson Collapsing from Within - Experia The Kuna Yala Battlefield - Christian Andersen Artlist: Tom Goldstein - Mirrors Jordan Hatfield - Sin and Death Grisly Brill - Squad Kill Quinten Coblentz - Babylon Theatre of Delays - Cosmos Ian Post - Where There’s Smoke Messwave - blink n miss - No Lead Vocals Ardie Son - Spoil the Mystery Alon Peretz - Fire and Ice Gal Lev - Empty Rooms Audiio: LivingForce - The Investigation Team (Instrumental) Lens Distortions: Knife's Edge Zero Sum No Half Measures The Perfect Fit _____ Armchair documentaries, almost weekly
</media:description>
<media:community>
<media:starRating count="106123" average="5.00" min="1" max="5"/>
<media:statistics views="5645108"/>
</media:community>
</media:group>
</entry>
</feed>"#;

    #[test]
    pub fn parse_xml_feed() {
        let feed: Feed = quick_xml::de::from_str(SAMPLE_FEED)
            .context("deserialize")
            .unwrap();

        assert_eq!(feed.channel_id, "ODHrzPMGbNv67e84WDZhQQ");
        assert_eq!(feed.title, "fern");
        assert_eq!(feed.entries.len(), 15);
    }
}
