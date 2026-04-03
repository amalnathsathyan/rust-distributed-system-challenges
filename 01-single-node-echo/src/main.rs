use std::io::Write;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

//Maelstrom protocol overview: https://github.com/jepsen-io/maelstrom/blob/main/doc/protocol.md
//Maelstrom nodes receive messages on STDIN, send messages on STDOUT
//A Maelstrom test simulates a distributed system by running many nodes, and a network which routes messages between them.
/*

Each message object is of the form:
{
  "src":  A string identifying the node this message came from
  "dest": A string identifying the node this message is to
  "body": An object: the payload of the message
}
*/
// Defining the layout of message that the maelstrom sends us

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Message {
    src: String,
    //Since dest is a common field in Maelstrom but sometimes avoided in Rust naming, this tells Serde to look for "dest" in JSON but call it dst in Rust.
    #[serde(rename = "dest")]
    dst: String,
    body: Body,
}

// each message has a body, any of the key-values from json can be stored in a hashmap
//alternative: we can have an enum for the extra payload
// `flatten` captures any other dynamic JSON fields.
// For an echo message, it will capture `"echo": "Please echo 35"`
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Body {
    #[serde(rename = "msg_id")]
    id: Option<usize>,
    in_reply_to: Option<usize>,
    #[serde(flatten)]
    payload: Payload,
}

//#[serde(tag = "type")] On an enum: Use the internally tagged enum representation, with the given tag
// Crucial: Maelstrom identifies message types (like "echo", "init", "read") using a "type": "..." field. This attribute tells Serde to look at that field to decide which variant of the Payload enum to use.
// Internal Tagging: Uses the "type" field to determine which variant to deserialize.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum Payload {
    Init {
        node_id: String,
        node_ids: Vec<String>,
    },
    InitOk,
    Echo {
        echo: String,
    },
    EchoOk {
        echo: String,
    },
}

fn main() -> Result<()> {
    //In Rust, std::io::stdin() returns a handle to the global standard input stream.
    //Without .lock(): Every time you call read_line or similar, Rust has to lock the input, read one piece of data, and then unlock it. If you are reading thousands of messages, this constant locking/unlocking is extremely slow.
    // By locking stdin, you get a type that implements the BufRead trait. This gives you access to very convenient methods for processing stream-based data,
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    //we can use serde_json:: Deserializer so that we can convert the input stream into and iterator
    // what it does?: Every time you ask for the "next" item in the iterator, it looks at the stream, finds the next valid JSON object, and attempts to turn it into your Message struct.
    // The iterator yields Result<Message, serde_json::Error>
    let inputs = serde_json::Deserializer::from_reader(stdin).into_iter::<Message>();

    // We start with a message ID counter for our independent responses
    let mut current_msg_id = 0;
    for input in inputs {
        let msg = input.context("Maelstrom input from STDIN could not be processed")?;
        // Logic: For every request we receive, we swap src/dst and prepare a response body
        // and increment our local message counter.
        current_msg_id += 1;
        let reply_payload = match msg.body.payload {
            Payload::Init { .. } => Payload::InitOk,
            Payload::Echo { echo } => Payload::EchoOk { echo },
            Payload::InitOk | Payload::EchoOk { .. } => continue, // We don't reply to ok's
        };
        let reply = Message {
            src: msg.dst, // Our node's ID (received as 'dest' in the request)
            dst: msg.src, // The client's ID (received as 'src' in the request)
            body: Body {
                id: Some(current_msg_id),
                in_reply_to: msg.body.id,
                payload: reply_payload,
            },
        };
        // Write the reply to stdout follow by a newline (required by the protocol)
        serde_json::to_writer(&mut stdout, &reply)?;
        stdout.write_all(b"\n")?;
    }
    Ok(())
}
