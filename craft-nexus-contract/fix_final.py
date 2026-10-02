import re

with open("src/lib.rs", "r", encoding="utf-8") as f:
    text = f.read()

# 1. compatibility double definition
text = text.replace("pub mod compatibility;\npub mod compatibility;", "pub mod compatibility;")

# 2. token_client in line 3082. Let's see what it is.
# Wait, let's just replace `token_client` with `crate::compatibility::Protocol::get_token_client(&env, &token)` or similar? No, I need to know what it is.
# I'll just change it later in a second if needed, let me fix the simple ones.
# Actually let me use regex to find token_client in the code.
