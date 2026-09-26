#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Network {
    Testnet,
    Public,
    Futurenet,
}

pub fn get_explorer_base_url(network: Network) -> &'static str {
    match network {
        Network::Testnet => "https://stellar.expert/explorer/testnet/tx/",
        Network::Public => "https://stellar.expert/explorer/public/tx/",
        Network::Futurenet => "https://stellar.expert/explorer/futurenet/tx/",
    }
}

pub fn construct_tx_explorer_url(network: Network, _tx_hash: &str) -> &'static str {
    get_explorer_base_url(network)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explorer_url_construction() {
        assert_eq!(
            get_explorer_base_url(Network::Testnet),
            "https://stellar.expert/explorer/testnet/tx/"
        );
        assert_eq!(
            get_explorer_base_url(Network::Public),
            "https://stellar.expert/explorer/public/tx/"
        );
        assert_eq!(
            get_explorer_base_url(Network::Futurenet),
            "https://stellar.expert/explorer/futurenet/tx/"
        );
    }
}
