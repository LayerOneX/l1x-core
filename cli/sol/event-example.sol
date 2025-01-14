pragma solidity ^0.8.0;

contract EventExample {
    event Event1(
        address indexed sender,
        string message,
        address receiver,
        uint indexed val
    );

    event Event2(address indexed sender, string message, uint indexed val);

    function emitEvent1() public {
        emit Event1(msg.sender, "Hello, world!", msg.sender, 10);
    }

    function emitEvent2() public {
        emit Event2(msg.sender, "Hi, world!", 50);
    }
}
