# Synthetic offline provider fixtures

These small JSON bodies contain invented transaction IDs, merchants and amounts.
They are not exports from payment accounts and do not describe the real WeChat,
Alipay, or any bank API. Their distinct envelope names exercise independent adapter
decoders over a common normalization/preview contract. Tests inject HTTP status,
rate-limit headers, network failures and repeated pages in memory; no network is used.
