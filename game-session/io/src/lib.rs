#![no_std]

use gmeta::{In, InOut, Metadata, Out};
use gstd::{collections::HashMap, prelude::*, ActorId, MessageId};

// 游戏会话元数据结构
pub struct GameSessionMetadata;

impl Metadata for GameSessionMetadata {
    type Init = In<GameSessionInit>;
    type Handle = InOut<GameSessionAction, GameSessionEvent>;
    type State = Out<GameSessionState>;
    type Reply = ();
    type Others = ();
    type Signal = ();
}

// 游戏会话状态
#[derive(Debug, Default, Clone, Encode, Decode, TypeInfo)]
pub struct GameSessionState {
    pub wordle_program_id: ActorId,
    pub active_sessions: Vec<(ActorId, SessionDetails)>,
}

// 游戏会话初始化参数
#[derive(Debug, Default, Clone, Encode, Decode, TypeInfo)]
pub struct GameSessionInit {
    pub wordle_contract_id: ActorId,
}

impl GameSessionInit {
    pub fn validate(&self) {
        assert!(!self.wordle_contract_id.is_zero(), "无效的Wordle合约ID");
    }
}

// 从初始化参数创建游戏会话
impl From<GameSessionInit> for GameSession {
    fn from(init: GameSessionInit) -> Self {
        Self {
            wordle_contract_id: init.wordle_contract_id,
            ..Default::default()
        }
    }
}

// 游戏会话动作
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum GameSessionAction {
    InitiateGame,
    VerifyGuess { guess: String },
    CheckSessionStatus { player: ActorId, session_id: MessageId },
}

// Wordle合约动作
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum WordleContractAction {
    InitiateGame { player: ActorId },
    VerifyGuess { player: ActorId, guess: String },
}

// 游戏会话事件
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum GameSessionEvent {
    GameInitiated,
    GuessResult {
        correct_positions: Vec<u8>,
        present_letters: Vec<u8>,
    },
    GameConcluded(GameOutcome),
}

// 游戏结果
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum GameOutcome {
    Victory,
    Defeat,
}

// Wordle合约事件
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum WordleContractEvent {
    GameInitiated {
        player: ActorId,
    },
    GuessVerified {
        player: ActorId,
        correct_positions: Vec<u8>,
        present_letters: Vec<u8>,
    },
}

impl WordleContractEvent {
    pub fn get_player(&self) -> &ActorId {
        match self {
            WordleContractEvent::GameInitiated { player } => player,
            WordleContractEvent::GuessVerified { player, .. } => player,
        }
    }

    pub fn is_correct_guess(&self) -> bool {
        match self {
            WordleContractEvent::GameInitiated { .. } => unreachable!(),
            WordleContractEvent::GuessVerified { correct_positions, .. } => {
                correct_positions == &vec![0, 1, 2, 3, 4]
            }
        }
    }
}

// 将Wordle合约事件转换为游戏会话事件
impl From<&WordleContractEvent> for GameSessionEvent {
    fn from(event: &WordleContractEvent) -> Self {
        match event {
            WordleContractEvent::GameInitiated { .. } => GameSessionEvent::GameInitiated,
            WordleContractEvent::GuessVerified {
                correct_positions,
                present_letters,
                ..
            } => GameSessionEvent::GuessResult {
                correct_positions: correct_positions.clone(),
                present_letters: present_letters.clone(),
            },
        }
    }
}

// 会话状态
#[derive(Default, Debug, Clone, Encode, Decode, TypeInfo)]
pub enum SessionState {
    #[default]
    Initialized,
    AwaitingPlayerInput,
    AwaitingWordleInitResponse,
    AwaitingWordleGuessResponse,
    ResponseReceived(WordleContractEvent),
    Concluded(GameOutcome),
}

// 会话详情
#[derive(Default, Debug, Clone, Encode, Decode, TypeInfo)]
pub struct SessionDetails {
    pub session_id: MessageId,
    pub original_msg_id: MessageId,
    pub wordle_msg_id: MessageId,
    pub attempt_count: u8,
    pub current_state: SessionState,
}

impl SessionDetails {
    pub fn is_awaiting_response(&self) -> bool {
        matches!(
            self.current_state,
            SessionState::AwaitingWordleGuessResponse | SessionState::AwaitingWordleInitResponse
        )
    }
}

// 游戏会话
#[derive(Default, Debug, Clone)]
pub struct GameSession {
    pub wordle_contract_id: ActorId,
    pub active_sessions: HashMap<ActorId, SessionDetails>,
}

// 将游戏会话转换为游戏会话状态
impl From<&GameSession> for GameSessionState {
    fn from(session: &GameSession) -> Self {
        Self {
            wordle_program_id: session.wordle_contract_id,
            active_sessions: session
                .active_sessions
                .iter()
                .map(|(k, v)| (*k, v.clone()))
                .collect(),
        }
    }
}
