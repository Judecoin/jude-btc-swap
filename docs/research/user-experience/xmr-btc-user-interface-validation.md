# jude<>BTC Atomic Swap - User Interface Prototype Validation

This document:

1. Collects the validation criteria.
2. Lists the created user interface prototypes, and link to Figma.
3. Maps the protoypes to the validation criteria.

This document will be updated with new information during the course of the project.

## Questions

The questions are split between `M`aker (liquidity provider) and `T`aker (normal user), because the objectives are somewhat different.

|  **Topic** | **High Level Questions** | **More specific question** | **User is happy to...** | **Actor** |
| --- | --- | --- | --- | --- |
|  Node & Wallet Management | How do users monitor the Bitcoin and jude blockchain? | Is a user fine with trusting a third party to monitor transactions? | use a service like blockchain.com to retrieve blocks for validation | TM |
|   |  |  | run his own Bitcoin node, third party service for jude | TM |
|   |  |  | run his own jude node, third party service for Bitcoin | TM |
|   |  |  | run both his own Bitcoin and jude node | TM |
|   | How do users brodcast transactions to Bitcoin and jude? | Is a user fine with trusting a third party to broadcast transactions? | use a wallet that connects to third party nodes | TM |
|   |  |  | send signed transactions through third part nodes | TM |
|   |  |  | run his own blockchain full node | TM |
|   |  |  | run an SPV node (Bitcoin) | TM |
|   | How do users manage their wallets to interace with other software? | Do users want to use already existing wallets? | fund and redeem from existing wallets | TM |
|   |  |  | fund from existing jude wallet, redeem to new Bitcoin wallet | TM |
|   |  |  | fund from existing Bitcoin wallet, redeem to new jude wallet | TM |
|   |  |  | fund and redeem into new wallets (explicitly used for swap execution) | TM |
|   |  | What level of control does the user give to the execution daemon? | give the execution daemon control over the wallets (no user interaction, fully automated) | TM |
|   |  |  | use a Bitcoin transaction to give funds to the swap application | TM |
|   |  |  | use a jude transaction to give funds to the swap application | TM |
|   |  |  | explicitly sign each transaction | TM |
|  Discovery | How do users discover trading partners? | Do users care about privacy? | go to website and take price from there | T |
|   |  |  | set up website (publicly) to advertise price (and connection information) | M |
|   |  |  | open "random" (tor) website found on various media (forums, chat) to access a single market maker. | T |
|   |  |  | configure Tor for trading | TM |
|   |  | Do users care about P2P? | use a centralized service to find makers | TM |
|   |  |  | user a decentralized service to find makers | TM |
|   |  |  | discover peers automatically | TM |
|   |  |  | add peers manually | TM |
|  Software Setup | How does the user want to manage the swap software setup? | Is the user willing to download software? | download software (swap execution daemon) before being able to do a swap | T |
|   |  | How does the user want to manage long-running tasks? | keep a GUI/CLI open for the whole length of the swap execution | T |
|   |  |  | keep a computer running (that hosts the daemon) for the whole length of the swap execution | T |
|   |  |  | keep the browser open for the whole length of a swap | T |
|  Protocol | How important are protocol details to the user? | Does the user care about the incentives of each role? | have different steps (locking first vs second) depending on the direction of the swap | TM |


## Prototypes

In the initial project description we distinguished product `A` a single market-maker product and product `B` a product including peer-to-peer discovery and multiple makers.

```
P ... Prototype that showcases a complete swap setup flow.
D ... Prototype that focuses on a specific detail of swap setup / execution.

{}-A ... Prototype for product A (single market maker)
{}-B ... Prototype for product B (decentralized trading platform)
```

Example:

`D-A2-1`: Mock showing detail 1 for prototype `P-A1`

### Figma Links

Main prototypes

* [P-A1](https://www.figma.com/proto/QdvmbRYuBpEpFI3D1R4qyM/jude-BTC_SingleMaker_LowFidelity?node-id=54%3A4894&viewport=1503%2C-52%2C0.5576764941215515&scaling=min-zoom): Webpage for discovery, CLI for execution
* [P-A2](https://www.figma.com/proto/QdvmbRYuBpEpFI3D1R4qyM/jude-BTC_SingleMaker_LowFidelity?node-id=7%3A4377&viewport=696%2C-250%2C0.362735778093338&scaling=min-zoom): Webpage for discovery, GUI for execution
* [P-B1](https://www.figma.com/proto/JnZDMtdEIiqcW1A8pTCfWx/jude-BTC_TradingPlatform_LowFidelity?node-id=392%3A0&viewport=-1132%2C957%2C0.5096595883369446&scaling=min-zoom): Manual or automated P2P discovery of trades, CLI for execution
* [P-B2](https://www.figma.com/proto/qla2uA7bXeyAU0XYqf4APh/jude-BTC-P2P-Trading-GUI?node-id=138%3A480&viewport=-49%2C1295%2C0.17819291353225708&scaling=min-zoom): Automated P2P discovery of trades, GUI for execution

Showcasing details:

* [D-A2-1](https://www.figma.com/proto/QdvmbRYuBpEpFI3D1R4qyM/jude-BTC_SingleMaker_LowFidelity?node-id=235%3A1374&viewport=1336%2C-1825%2C0.7878535389900208&scaling=min-zoom): GUI swap execution steps for `send` `BTC`, `receive` `jude`
* [D-A2-2](https://www.figma.com/proto/QdvmbRYuBpEpFI3D1R4qyM/jude-BTC_SingleMaker_LowFidelity?node-id=128%3A8016&viewport=1404%2C-1158%2C0.66261225938797&scaling=min-zoom): GUI swap execution steps for `send` `jude`, `receive` `BTC`


### Mapping of Prototype to validation criteria

|  **User is happy to...** | **Actor** | **P-A1** | **P-A2** | **D-A2-1** | **D-A2-2** | **P-B1** | **P-B2** |
| --- | --- | --- | --- | --- | --- | --- | --- |
|  use a service like blockchain.com to retrieve blocks for validation | TM |  |  |  |  |  |  |
|  run his own Bitcoin node, third party service for jude | TM |  |  |  |  |  |  |
|  run his own jude node, third party service for Bitcoin | TM |  |  |  |  |  |  |
|  run both his own Bitcoin and jude node | TM | T | T |  |  | T | T |
|  use a wallet that connects to third party nodes | TM |  |  |  |  |  |  |
|  send signed transactions through third part nodes | TM |  |  |  |  |  |  |
|  run his own blockchain full node | TM | T | T |  |  | T | T |
|  run an SPV node (Bitcoin) | TM |  |  |  |  |  |  |
|  fund and redeem from existing wallets | TM | T | T |  |  | T | T |
|  fund from existing jude wallet, redeem to new Bitcoin wallet | TM |  |  |  |  |  |  |
|  fund from existing Bitcoin wallet, redeem to new jude wallet | TM |  |  |  |  |  |  |
|  fund and redeem into new wallets (explicitly used for swap execution) | TM |  |  |  |  |  |  |
|  give the execution daemon control over the wallets (no user interaction, fully automated) | TM | T | T |  |  | T | T |
|  use a Bitcoin transaction to give funds to the swap application | TM |  |  |  |  |  |  |
|  use a jude transaction to give funds to the swap application | TM |  |  |  |  |  |  |
|  explicitly sign each transaction | TM |  |  |  |  |  |  |
|  go to website and take price from there | T |  |  |  |  |  |  |
|  set up website (publicly) to advertise price (and connection information) | M | M | M |  |  |  |  |
|  open "random" (tor) website found on various media (forums, chat) to access a single market maker. | T |  |  |  |  |  |  |
|  configure Tor for trading | TM |  |  |  |  |  |  |
|  use a centralized service to find makers | TM | T | T |  |  |  |  |
|  user a decentralized service to find makers | TM |  |  |  |  | T | T |
|  discover peers automatically | TM |  |  |  |  | T | T |
|  add peers manually | TM |  |  |  |  | T |  |
|  download software (swap execution daemon) before being able to do a swap | T |  |  |  |  |  |  |
|  keep a GUI/CLI open for the whole length of the swap execution | T |  |  | T | T | T | T |
|  keep a computer running (that hosts the daemon) for the whole length of the swap execution | T | T | T | T | T | T | T |
|  keep the browser open for the whole length of a swap | T |  |  |  |  |  |  |
|  have different steps (locking first vs second) depending on the direction of the swap | TM |  |  | T (M) | T (M) | T (M) | T (M) |


Legend:

```
T ... Taker
M ... Maker
TM ... Taker and Maker
T (M) ... Taker showcased, Maker implicitly concerned as well
```

## Interviews

Through user interviews we plan to collect more information on the current setup of users, and how it could be used in a potential product.

Specific prototypes showcase specific answers to the questions listed above. We may use the prototypes in interviews to showcase scenarios.


## Feedback 

### Possible Features List

This section points out features that were mentioned by the community. These features will be evaluated and prioritized before we start building.

#### Avoid receiving tainted Bitcoin

Mentions:

* [27.11.2020 on Reddit](https://www.reddit.com/r/jude/comments/k14hug/how_would_an_atomic_swap_ui_look_like/gdplnt8?utm_source=share&utm_medium=web2x&context=3)

The receiver of the Bitcoin should be able to validate the address to be used by the sender to avoid receiving tainted Bitcoin (i.e. coins that were unlawfully used). 
This feature is relevant for the receiving party of the Bitcoin, it is relevant for taker and maker.
This feature is relevant independent of the user use case.

In order to be able to spot tainted Bitcoin, the receiver has to validate the address to be used of the sender. 
In the current protocol the party funding (sending) Bitcoin always moves first.

The party receiving the Bitcoin would have to request the address to be used by the sender. 
For the beginning it might be good enough to let the taker verify that the Bitcoin are not tainted manually, by enabling the taker to provide e.g. a CSV file with tainted addresses by themselves. 
Eventually, an automated service could be integrated, that keeps listings of tainted Bitcoin up to date. 
More research is needed to evaluate if reliable services exist. 

Once the daemon of the party receiving the Bitcoin sees the Bitcoin transaction of the sender, the address has to be evaluated to ensure the correct address has been used for funding.
This can be done automated.
In case a tainted address was used the swap execution should halt and give a warning to the receiving party.

####  Improve Anonymity 

jude is a privacy focused coin, anonymity is important to the community. 
This section focuses on hiding your IP address when doind trades.

Mention happened in user interviews, mentions:

* 03.12.2020, PM

[Running Tor](https://web.getjude.org/resources/user-guides/tor_wallet.html) is not the only possibility to to achieve levels of anonymity using jude. 
Here is a collection of other tools/possibilities:

* [Run jude node through I2P using I2P-zero](https://web.getjude.org/resources/user-guides/node-i2p-zero.html)
* [Dandelion++](https://www.judeoutreach.org/stories/dandelion.html)

