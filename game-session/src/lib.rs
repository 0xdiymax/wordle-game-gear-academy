#![no_std]
use game_session_io::*;
use gstd::{exec, msg, debug, MessageId, ActorId, string::{String}};

const MAX_ATTEMPTS: u8 = 5;

static mut GAME_SESSION: Option<GameSession> = None;

fn get_game_session_mut() -> &'static mut GameSession {
    unsafe { GAME_SESSION.as_mut().expect("游戏会话未初始化") }
}

fn get_game_session() -> &'static GameSession {
    unsafe { GAME_SESSION.as_ref().expect("游戏会话未初始化") }
}

#[no_mangle]
extern "C" fn init() {
    debug!("初始化游戏会话");
    let init_params: GameSessionInit = msg::load().expect("无法解码初始化参数");
    init_params.validate();
    unsafe {
        GAME_SESSION = Some(init_params.into());
    };
    debug!("游戏会话初始化完成");
}

#[no_mangle]
extern "C" fn handle() {
    debug!("处理游戏会话动作");
    let action: GameSessionAction = msg::load().expect("无法解码游戏会话动作");
    let session = get_game_session_mut();
    
    match action {
        GameSessionAction::InitiateGame => handle_initiate_game(session),
        GameSessionAction::VerifyGuess { guess } => handle_verify_guess(session, guess),
        GameSessionAction::CheckSessionStatus { player, session_id } => {
            handle_check_session_status(session, player, session_id)
        }
    }
    
    debug!("游戏会话动作处理完成");
}

fn handle_initiate_game(session: &mut GameSession) {
    let player = msg::source();
    debug!("玩家 {:?} 请求开始新游戏", player);
    
    let session_details = session.active_sessions.entry(player).or_default();
    match &session_details.current_state {
        SessionState::ResponseReceived(event) => {
            msg::reply::<GameSessionEvent>(event.into(), 0).expect("回复失败");
            session_details.current_state = SessionState::AwaitingPlayerInput;
            debug!("游戏已开始,等待玩家输入");
        }
        SessionState::Initialized | SessionState::Concluded(..) | SessionState::AwaitingWordleInitResponse => {
            let wordle_msg_id = msg::send(
                session.wordle_contract_id,
                WordleContractAction::InitiateGame { player },
                0,
            ).expect("发送消息失败");

            session_details.session_id = msg::id();
            session_details.original_msg_id = msg::id();
            session_details.wordle_msg_id = wordle_msg_id;
            session_details.attempt_count = 0;
            session_details.current_state = SessionState::AwaitingWordleInitResponse;
            
            msg::send_delayed(
                exec::program_id(),
                GameSessionAction::CheckSessionStatus {
                    player,
                    session_id: msg::id(),
                },
                0,
                200,
            ).expect("发送延迟消息失败");
            
            debug!("等待Wordle合约响应");
            exec::wait();
        }
        _ => {
            debug!("玩家已在游戏中");
            panic!("玩家已在游戏中");
        }
    }
}

fn handle_verify_guess(session: &mut GameSession, guess: String) {
    let player = msg::source();
    debug!("处理玩家 {:?} 的猜测: {}", player, guess);
    
    let session_details = session.active_sessions.entry(player).or_default();
    match &session_details.current_state {
        SessionState::ResponseReceived(event) => {
            session_details.attempt_count += 1;
            debug!("当前尝试次数: {}", session_details.attempt_count);
            
            if event.is_correct_guess() {
                session_details.current_state = SessionState::Concluded(GameOutcome::Victory);
                msg::reply(GameSessionEvent::GameConcluded(GameOutcome::Victory), 0)
                    .expect("回复失败");
                debug!("玩家猜对了单词");
            } else if session_details.attempt_count == MAX_ATTEMPTS {
                session_details.current_state = SessionState::Concluded(GameOutcome::Defeat);
                msg::reply(GameSessionEvent::GameConcluded(GameOutcome::Defeat), 0)
                    .expect("回复失败");
                debug!("玩家用完了所有尝试机会");
            } else {
                let guess_result = GameSessionEvent::from(event);
                debug!("返回猜测结果: {:?}", guess_result);
                msg::reply(guess_result, 0).expect("回复失败");
                session_details.current_state = SessionState::AwaitingPlayerInput;
            }
        }
        SessionState::AwaitingPlayerInput | SessionState::AwaitingWordleGuessResponse => {
            assert!(
                guess.len() == 5 && guess.chars().all(|c| c.is_lowercase()),
                "无效的猜测"
            );
            let wordle_msg_id = msg::send(
                session.wordle_contract_id,
                WordleContractAction::VerifyGuess { player, guess },
                0,
            ).expect("发送消息失败");
            session_details.original_msg_id = msg::id();
            session_details.wordle_msg_id = wordle_msg_id;
            session_details.current_state = SessionState::AwaitingWordleGuessResponse;
            debug!("等待Wordle合约验证猜测");
            exec::wait();
        }
        _ => {
            debug!("玩家不在游戏中");
            panic!("玩家不在游戏中");
        }
    }
}

fn handle_check_session_status(session: &mut GameSession, player: ActorId, session_id: MessageId) {
    if msg::source() == exec::program_id() {
        debug!("检查玩家 {:?} 的会话状态", player);
        if let Some(session_details) = session.active_sessions.get_mut(&player) {
            if session_id == session_details.session_id
                && !matches!(session_details.current_state, SessionState::Concluded(..))
            {
                session_details.current_state = SessionState::Concluded(GameOutcome::Defeat);
                msg::send(player, GameSessionEvent::GameConcluded(GameOutcome::Defeat), 0)
                    .expect("发送消息失败");
                debug!("游戏超时,玩家失败");
            }
        }
    }
}

#[no_mangle]
extern "C" fn handle_reply() {
    debug!("处理Wordle合约回复");
    let reply_to = msg::reply_to().expect("无法获取回复来源");
    let event: WordleContractEvent = msg::load().expect("无法解码Wordle合约事件");
    let session = get_game_session_mut();
    let player = event.get_player();
    if let Some(session_details) = session.active_sessions.get_mut(player) {
        if reply_to == session_details.wordle_msg_id && session_details.is_awaiting_response() {
            session_details.current_state = SessionState::ResponseReceived(event);
            exec::wake(session_details.original_msg_id).expect("唤醒消息失败");
            debug!("已处理Wordle合约回复并唤醒原始消息");
        }
    }
}

#[no_mangle]
extern "C" fn state() {
    debug!("获取游戏会话状态");
    let session = get_game_session();
    msg::reply::<GameSessionState>(session.into(), 0)
        .expect("回复状态失败");
}